#!/usr/bin/env python3
"""30c — monitor-environment replication harness (Phase 30, plan 30-02).

Answers ONE question: does the Claude CLI still deliver
``task-notification``-origin ``result`` events when it is NOT running inside an
interactive Claude Code session, but inside a replica of the process
environment ``spawn_monitor`` imposes in production?

All three archived 30a captures ran inside an interactive session (inherited
environment, merged stderr, a pipe the harness owned end to end). Review
finding M4 named that as the deciding untested variable, and the answer gates
whether Phase 31 is planned at all.

WHY A REPLICA AND NOT ``spawn_monitor`` ITSELF
----------------------------------------------
``spawn_monitor`` configures its child with ``.stdin(Stdio::null())``
(``crates/devflow-core/src/monitor.rs:171``). The whole experiment depends on
holding stdin OPEN past the first ``result`` event — that is what keeps the
session alive — so a run through an unmodified ``spawn_monitor`` could not
perform the measurement at all, and modifying it is forbidden by this phase's
locked decision. This harness therefore isolates the ENVIRONMENT variable and
leaves pipe ownership (Phase 31's separate question) alone.

WHAT IS REPLICATED (five characteristics, each a real interactive/production
difference)
    1. Launch through ``sh -c`` with production's script shape, not a direct
       ``claude`` exec — mirrors ``hermetic_command("sh", workdir).arg("-c")``
       at ``monitor.rs:162-174``.
    2. Environment scrubbed of every variable named in ``REPO_LOCAL_GIT_VARS``
       and ``ALSO_REDIRECTING_GIT_VARS``, PARSED OUT OF
       ``crates/devflow-core/src/git.rs`` at runtime. Never transcribed: a
       stale copy silently tests an environment production does not impose.
    3. Detached — ``start_new_session=True``, own process group, no
       controlling terminal.
    4. No TTY on any stream.
    5. stderr redirected to its own file, never merged into stdout.

THE SINGLE DELIBERATE DEVIATION
    The harness owns and holds open the child's **stdin** pipe. That is the
    variable under test. See ``_monitor_script`` for the one line of shell
    that implements it and why it cannot be implicit.

EVIDENCE HANDLING
    Raw stdout/stderr are staged in a run-scoped temporary directory OUTSIDE
    ``.planning/``. Nothing reaches the committed evidence directory except
    through :func:`publish_jsonl` / :func:`publish_text`, which validate,
    structurally redact, secret-scan and only then atomically replace. An
    OS-level redirection cannot be intercepted by a redactor, so the only way
    redaction can precede a committed write is for the committed write to be a
    separate, later step.

This module reports OBSERVATIONS ONLY. It deliberately prints no verdict:
30a's README records that the harness-printed verdict was wrong in v1 and
unreliable in v2, and that the raw JSONL is the evidence of record.

Safe to import — nothing runs at import time.
"""

import json
import os
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from typing import Any, Callable, Iterable, Optional

HARNESS_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HARNESS_DIR, os.pardir, os.pardir, os.pardir))
GIT_RS_PATH = os.path.join(REPO_ROOT, "crates", "devflow-core", "src", "git.rs")
EVIDENCE_DIR = os.path.join(HARNESS_DIR, "30c-evidence")

#: Trial 2 publishes here. Trial 1's evidence is never overwritten — the
#: contaminated run is the comparison arm that makes trial 2 interpretable.
EVIDENCE_DIR_SCRUBBED = os.path.join(HARNESS_DIR, "30c-evidence-scrubbed")

#: The replication set publishes to `trial-{n}/` beneath this. Trials 1 and 2
#: are never overwritten.
EVIDENCE_DIR_RELIABILITY = os.path.join(HARNESS_DIR, "30c-evidence-reliability")

#: Planted in the parent before each replication trial so production's git
#: scrub does real work DURING the measured run, not only in a separate
#: mechanism check. Both point at paths that do not exist: if the scrub ever
#: failed to remove them, git inside the child would fail loudly rather than
#: silently acting on the wrong repository.
RELIABILITY_DECOY_GIT_ENV = {
    "GIT_DIR": "/tmp/devflow-30c-decoy-git-dir",
    "GIT_WORK_TREE": "/tmp/devflow-30c-decoy-work-tree",
}

#: The const names whose string literals form production's env-scrub list.
SCRUB_CONST_NAMES = ("REPO_LOCAL_GIT_VARS", "ALSO_REDIRECTING_GIT_VARS")

#: Every archived capture's ``init`` event reports this. A different CLI makes
#: comparison against 30a's baseline meaningless, so the run refuses to start.
EXPECTED_CLI_VERSION = "2.1.220"

CLAUDE_MARKER_PATTERN = re.compile(r"^(CLAUDE|CLAUDECODE|ANTHROPIC|AI_AGENT)", re.IGNORECASE)
CREDENTIAL_NAME_PATTERN = re.compile(r"KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL", re.IGNORECASE)


class HarnessError(RuntimeError):
    """A precondition or replication guarantee this harness cannot honour."""


class SecretScanError(HarnessError):
    """The secret scan matched. Nothing was published."""


# ---------------------------------------------------------------------------
# Environment replication
# ---------------------------------------------------------------------------


def parse_git_scrub_vars(git_rs_path: str = GIT_RS_PATH) -> list[str]:
    """Return the env-var names production scrubs, parsed from live source.

    Reads the two ``pub const``s out of ``git.rs`` rather than carrying a copy
    (30-RESEARCH.md Pattern 3). Raises rather than returning a short list: an
    empty scrub is the failure mode that looks like success.
    """
    try:
        with open(git_rs_path, encoding="utf-8") as handle:
            source = handle.read()
    except OSError as exc:
        raise HarnessError(
            f"cannot read the env-scrub source {git_rs_path!r}: {exc}. "
            "Refusing to run with a hardcoded or empty scrub list — the "
            "harness would silently measure an environment production does "
            "not impose."
        ) from exc

    names: list[str] = []
    for const_name in SCRUB_CONST_NAMES:
        block = re.search(
            rf"pub const {const_name}:\s*&\[&str\]\s*=\s*&\[(.*?)\];", source, re.S
        )
        if block is None:
            raise HarnessError(
                f"const {const_name} not found in {git_rs_path!r} — the scrub "
                "list cannot be derived, so the replication guarantee fails."
            )
        found = re.findall(r'"([A-Za-z0-9_]+)"', block.group(1))
        if not found:
            raise HarnessError(
                f"const {const_name} yielded ZERO variables from "
                f"{git_rs_path!r}. Aborting: an empty scrub list looks like a "
                "successful run while testing the wrong environment."
            )
        names.extend(found)
    return names


def build_child_env(
    scrub_names: Iterable[str], base: Optional[dict[str, str]] = None
) -> tuple[dict[str, str], list[str]]:
    """Return (child environment, names actually removed from it)."""
    env = dict(os.environ if base is None else base)
    removed = [name for name in scrub_names if env.pop(name, None) is not None]
    return env, removed


def discover_agent_session_markers(env: Optional[dict[str, str]] = None) -> list[str]:
    """Every ``CLAUDE*`` / ``ANTHROPIC*`` / ``AI_AGENT*`` name present.

    Trial 1 left these in place and recorded them, because production's env
    scrub (the ``git.rs`` lists) does not touch them. That left one hypothesis
    alive: ``CLAUDECODE`` / ``CLAUDE_CODE_ENTRYPOINT`` could plausibly trigger
    nested-session detection, so trial 1 would have proven delivery works
    INSIDE an agent session rather than in production's plain shell.

    Trial 2 removes them to kill that hypothesis. Credential-named members
    (``ANTHROPIC_API_KEY``, ``ANTHROPIC_TOKEN``) are NOT special-cased here:
    both this run's and the 30a baseline's ``init`` events report
    ``apiKeySource: "none"``, i.e. the CLI authenticates from stored
    credentials and is already ignoring those variables. If removing them
    nonetheless breaks authentication, that is a reportable result — not
    something to quietly restore.
    """
    env = os.environ if env is None else env
    return sorted(name for name in env if CLAUDE_MARKER_PATTERN.match(name))


def describe_env_residue(env: dict[str, str]) -> dict[str, Any]:
    """Name — never value — what the scrub could not remove.

    ``start_new_session`` detaches a process; it does not sanitise inherited
    environment or erase ancestry. Recording the residue by name is what turns
    "one controlled variable" from an assertion into something a reader can
    check (review finding: "exactly one controlled variable" is unproven).

    Credential-named variables are counted, not named (T-30-07).
    """
    markers, credential_named = [], []
    for name in sorted(env):
        if CREDENTIAL_NAME_PATTERN.search(name):
            credential_named.append(name)
        elif CLAUDE_MARKER_PATTERN.match(name):
            markers.append(name)
    return {
        "claude_session_markers": markers,
        "credential_named_var_count": len(credential_named),
        "total_env_vars": len(env),
        "parent_process": _parent_process_name(),
        "launched_from_agent_session": "CLAUDECODE" in env or "CLAUDE_CODE_ENTRYPOINT" in env,
    }


def _parent_process_name() -> str:
    try:
        with open(f"/proc/{os.getppid()}/comm", encoding="utf-8") as handle:
            return handle.read().strip()
    except OSError:
        return "unknown"


def _monitor_script(
    workdir: str, stdout_path: str, stderr_path: str, pid_path: str, exit_path: str
) -> str:
    """Production's ``sh -c`` script (``monitor.rs:134-146``), plus ONE change.

    THE SINGLE DELIBERATE DEVIATION FROM ``spawn_monitor`` IS THE ``exec 3<&0``
    / ``<&3`` PAIR BELOW. It cannot be left implicit: POSIX requires a
    non-interactive shell to assign an asynchronous list's stdin from
    /dev/null "before any explicit redirections", so a bare ``cmd &`` would
    hand the CLI a closed stdin and the experiment could not run at all.
    Duplicating the harness-owned pipe to fd 3 and redirecting the background
    job from it is an EXPLICIT redirection, which overrides that default.

    Everything else — the trap, the cd, the background-and-wait, the pid and
    exit files, the separated ``>``/``2>`` targets — mirrors production.
    """
    return (
        "apid=''; cleanup() {{ [ -n \"$apid\" ] && kill \"$apid\" 2>/dev/null; exit 0; }}; "
        "trap cleanup TERM INT; "
        "cd {workdir} || exit 1; "
        # === single deliberate deviation from spawn_monitor: stdin held open ===
        "exec 3<&0; "
        '"$@" <&3 > {stdout_file} 2>{stderr_file} & '
        # === end deviation ===
        "apid=$!; echo $apid > {pid_file}; "
        "wait $apid; echo $? > {exit_file}"
    ).format(
        workdir=shlex.quote(workdir),
        stdout_file=shlex.quote(stdout_path),
        stderr_file=shlex.quote(stderr_path),
        pid_file=shlex.quote(pid_path),
        exit_file=shlex.quote(exit_path),
    )


DEFAULT_CLI_ARGV = [
    "claude",
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--dangerously-skip-permissions",
]


@dataclass
class LaunchedRun:
    """A live child plus every path it is writing to.

    Plan 30-04 imports :func:`launch_in_monitor_env` and consumes this rather
    than reimplementing the launch against ``run_experiment_v3.py``'s
    direct-exec shape.
    """

    proc: subprocess.Popen
    stage_dir: str
    stdout_path: str
    stderr_path: str
    pid_path: str
    exit_path: str
    workdir: str
    argv: list[str]
    scrubbed_vars: list[str]
    residue: dict[str, Any]
    started_at: float
    #: Agent-session markers removed beyond the git list (trial 2 only).
    scrubbed_markers: list[str] = field(default_factory=list)


def make_stage_dir() -> str:
    """A run-scoped directory for raw output, asserted outside the repository."""
    stage_dir = tempfile.mkdtemp(prefix="devflow-30c-")
    resolved = os.path.realpath(stage_dir)
    if resolved.startswith(os.path.realpath(REPO_ROOT) + os.sep):
        raise HarnessError(
            f"staging directory {resolved!r} resolved INSIDE the repository. "
            "Raw, unsanitised CLI output must never be written to a path that "
            "can be committed."
        )
    return stage_dir


def launch_in_monitor_env(
    workdir: str,
    prompt: str,
    argv: Optional[list[str]] = None,
    stage_dir: Optional[str] = None,
    git_rs_path: str = GIT_RS_PATH,
    extra_scrub_names: Optional[Iterable[str]] = None,
) -> LaunchedRun:
    """Launch the CLI inside a replica of ``spawn_monitor``'s environment.

    Returns the launched process together with the staged paths it writes to.
    The caller owns ``proc.stdin`` and is responsible for closing it — holding
    it open is the variable under test.

    ``extra_scrub_names`` removes variables BEYOND production's git lists. It
    is empty by default, so the production replica is unchanged and plan
    30-04's reuse is unaffected; trial 2 passes the agent-session markers
    through it to test whether those markers were carrying the trial-1 result.
    """
    stage_dir = stage_dir or make_stage_dir()
    paths = {
        name: os.path.join(stage_dir, filename)
        for name, filename in (
            ("stdout_path", "raw_stdout.jsonl"),
            ("stderr_path", "raw_stderr.log"),
            ("pid_path", "agent.pid"),
            ("exit_path", "agent.exit"),
        )
    }
    script = _monitor_script(
        workdir, paths["stdout_path"], paths["stderr_path"], paths["pid_path"], paths["exit_path"]
    )
    scrub_names = parse_git_scrub_vars(git_rs_path)
    extra = sorted(set(extra_scrub_names or ()))
    child_env, removed_all = build_child_env(list(scrub_names) + extra)
    removed = [name for name in removed_all if name not in extra]
    removed_markers = [name for name in removed_all if name in extra]
    argv = list(argv or DEFAULT_CLI_ARGV)

    started_at = time.time()
    proc = subprocess.Popen(
        ["sh", "-c", script, "sh", *argv],
        cwd=workdir,
        env=child_env,
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,  # production: Stdio::null()
        stderr=subprocess.DEVNULL,  # production: Stdio::null()
        start_new_session=True,  # detached, own process group, no controlling TTY
        text=True,
        bufsize=1,
    )
    proc.stdin.write(json.dumps({"type": "user", "message": {"role": "user", "content": prompt}}) + "\n")
    proc.stdin.flush()

    return LaunchedRun(
        proc=proc,
        stage_dir=stage_dir,
        workdir=workdir,
        argv=argv,
        scrubbed_vars=removed,
        scrubbed_markers=removed_markers,
        residue=describe_env_residue(child_env),
        started_at=started_at,
        **paths,
    )


# ---------------------------------------------------------------------------
# Observation (experiment core, inherited from run_experiment_v3.py)
# ---------------------------------------------------------------------------


class StagedTail:
    """Incremental reader over the staged stdout file.

    ``run_experiment_v3.py`` used ``select()`` on a pipe. Production's capture
    path is a FILE written by the ``sh`` script's own redirection and read back
    — ``select()`` on a regular file is always-ready and carries no
    information, so the same never-block guarantee is kept with a bounded poll
    plus partial-line buffering.
    """

    def __init__(self, path: str) -> None:
        self._path = path
        self._handle = None
        self._buffer = ""

    def poll_lines(self) -> list[str]:
        if self._handle is None:
            if not os.path.exists(self._path):
                return []
            self._handle = open(self._path, encoding="utf-8", errors="replace")
        chunk = self._handle.read()
        if not chunk:
            return []
        self._buffer += chunk
        *complete, self._buffer = self._buffer.split("\n")
        return [line for line in complete if line.strip()]

    def close(self) -> None:
        if self._handle is not None:
            self._handle.close()
            self._handle = None


@dataclass
class Observations:
    """Facts read off the stream. No interpretation, no verdict."""

    results: list[dict[str, Any]] = field(default_factory=list)
    notifications: list[dict[str, Any]] = field(default_factory=list)
    background_task_changes: list[dict[str, Any]] = field(default_factory=list)
    signals: dict[str, Optional[float]] = field(default_factory=dict)
    timeline: list[tuple[str, float]] = field(default_factory=list)
    exit_delay_after_stdin_close: Optional[float] = None
    exit_code: Optional[int] = None
    killed: bool = False
    duration_s: Optional[float] = None
    signal_contents: dict[str, Optional[str]] = field(default_factory=dict)

    def mark(self, label: str, elapsed: float) -> None:
        self.timeline.append((label, elapsed))


def _record_event(obs: Observations, raw_line: str, elapsed: float) -> None:
    try:
        event = json.loads(raw_line)
    except json.JSONDecodeError:
        return
    if not isinstance(event, dict):
        return
    kind, subtype = event.get("type"), event.get("subtype")
    if kind == "result":
        obs.results.append(
            {
                "at": round(elapsed, 2),
                "origin": event.get("origin"),
                "num_turns": event.get("num_turns"),
                "is_error": event.get("is_error"),
                "parent_tool_use_id": event.get("parent_tool_use_id"),
                "result_text": event.get("result"),
            }
        )
        obs.mark(f"result#{len(obs.results)}", elapsed)
    elif subtype == "task_notification":
        obs.notifications.append(
            {"at": round(elapsed, 2), "task_id": event.get("task_id"), "status": event.get("status")}
        )
    elif subtype == "background_tasks_changed":
        tasks = event.get("tasks") or []
        obs.background_task_changes.append(
            {
                "at": round(elapsed, 2),
                "count": len(tasks),
                "task_ids": [t.get("task_id") for t in tasks if isinstance(t, dict)],
            }
        )


def _poll_signals(obs: Observations, signal_paths: dict[str, str], elapsed: float) -> None:
    for label, path in signal_paths.items():
        if obs.signals.get(label) is None and os.path.exists(path):
            obs.signals[label] = round(elapsed, 2)
            print(f"[harness] t+{elapsed:5.1f} signal {label} written", flush=True)


def observe_run(
    run: LaunchedRun,
    signal_paths: dict[str, str],
    expected_results: int = 3,
    read_deadline: float = 60.0,
    straggler_window: float = 5.0,
    exit_window: float = 20.0,
    on_event: Optional[Callable[[dict[str, Any]], None]] = None,
) -> Observations:
    """Drive the run to completion. Never blocks indefinitely."""
    obs = Observations(signals={label: None for label in signal_paths})
    tail = StagedTail(run.stdout_path)
    obs.mark("launch", 0.0)

    while time.time() - run.started_at < read_deadline:
        elapsed = time.time() - run.started_at
        _poll_signals(obs, signal_paths, elapsed)
        lines = tail.poll_lines()
        for line in lines:
            _record_event(obs, line, time.time() - run.started_at)
            if on_event is not None:
                on_event({"line": line})
        if not lines:
            if run.proc.poll() is not None:
                print(f"[harness] t+{elapsed:5.1f} sh exited (code={run.proc.returncode})", flush=True)
                break
            time.sleep(0.25)
            continue
        _poll_signals(obs, signal_paths, time.time() - run.started_at)
        if len(obs.results) >= expected_results and all(v is not None for v in obs.signals.values()):
            print(f"[harness] t+{elapsed:5.1f} bar met; draining stragglers", flush=True)
            _drain(tail, obs, run, signal_paths, straggler_window)
            break

    _close_stdin_and_wait(tail, obs, run, signal_paths, exit_window)
    tail.close()
    return obs


def _drain(
    tail: StagedTail,
    obs: Observations,
    run: LaunchedRun,
    signal_paths: dict[str, str],
    window: float,
) -> None:
    stop_at = time.time() + window
    while time.time() < stop_at:
        for line in tail.poll_lines():
            _record_event(obs, line, time.time() - run.started_at)
        _poll_signals(obs, signal_paths, time.time() - run.started_at)
        time.sleep(0.25)


def _terminate_process_group(proc: subprocess.Popen) -> None:
    """SIGTERM the detached group first so production's own cleanup trap runs.

    ``start_new_session=True`` puts ``sh`` in its own group; killing only
    ``sh`` would orphan the CLI it is waiting on. SIGTERM fires the script's
    ``trap cleanup TERM INT``, which kills the agent pid; SIGKILL is the
    fallback if that does not land.
    """
    for signum, grace in ((signal.SIGTERM, 3.0), (signal.SIGKILL, 2.0)):
        if proc.poll() is not None:
            return
        try:
            os.killpg(os.getpgid(proc.pid), signum)
        except (ProcessLookupError, PermissionError):
            return
        deadline = time.time() + grace
        while time.time() < deadline and proc.poll() is None:
            time.sleep(0.1)
    proc.wait()


def _close_stdin_and_wait(
    tail: StagedTail,
    obs: Observations,
    run: LaunchedRun,
    signal_paths: dict[str, str],
    exit_window: float,
) -> None:
    elapsed = time.time() - run.started_at
    print(f"[harness] t+{elapsed:5.1f} closing stdin", flush=True)
    obs.mark("stdin_close", elapsed)
    closed_at = time.time()
    try:
        run.proc.stdin.close()
    except OSError:
        pass

    deadline = closed_at + exit_window
    while time.time() < deadline:
        for line in tail.poll_lines():
            _record_event(obs, line, time.time() - run.started_at)
        _poll_signals(obs, signal_paths, time.time() - run.started_at)
        if run.proc.poll() is not None:
            obs.exit_delay_after_stdin_close = round(time.time() - closed_at, 2)
            break
        time.sleep(0.25)

    if obs.exit_delay_after_stdin_close is None:
        obs.killed = True
        _terminate_process_group(run.proc)
    obs.exit_code = run.proc.returncode
    obs.mark("process_exit", time.time() - run.started_at)
    for line in tail.poll_lines():
        _record_event(obs, line, time.time() - run.started_at)


# ---------------------------------------------------------------------------
# Sanitisation and publication
# ---------------------------------------------------------------------------

_TOKEN_PATTERNS: tuple[tuple[str, re.Pattern], ...] = (
    ("openai_key_prefix", re.compile(r"\bsk-[A-Za-z0-9_\-]{8,}")),
    ("github_token_prefix", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{8,}")),
    ("bearer_token", re.compile(r"\bBearer\s+[A-Za-z0-9._~+/\-]{8,}={0,2}")),
    (
        "credential_named_assignment",
        re.compile(
            r"""["']?[A-Za-z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD)[A-Za-z0-9_]*["']?\s*[:=]\s*"""
            # already sanitised by the structural redactor
            r"""(?!["']?<redacted:)"""
            # a bare number is not a credential — `cache_read_input_tokens: 1234567`
            # is a usage counter, and treating it as a leak would fail the publish
            # of an entirely clean capture
            r"""(?!["']?[0-9.]+(?:["'\s,}\]]|$))"""
            r"""["']?[^\s"',}\]]{6,}""",
            re.IGNORECASE,
        ),
    ),
)

_UUID_PATTERN = re.compile(r"\b[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\b")


@dataclass
class RedactionContext:
    """Everything the scrubber replaces, resolved once per publish."""

    home_paths: list[str]
    username: str
    session_pseudonyms: dict[str, str] = field(default_factory=dict)

    @classmethod
    def build(cls, session_ids: Iterable[str] = ()) -> "RedactionContext":
        home = os.path.expanduser("~")
        candidates = {home, os.path.realpath(home)}
        # Fedora Silverblue exposes the same home under both /home and /var/home;
        # a capture can carry either spelling.
        if home.startswith("/var/home/"):
            candidates.add(home.replace("/var/home/", "/home/", 1))
        elif home.startswith("/home/"):
            candidates.add(home.replace("/home/", "/var/home/", 1))
        ctx = cls(
            home_paths=sorted(candidates, key=len, reverse=True),
            username=os.path.basename(home),
        )
        for session_id in session_ids:
            ctx.pseudonym(session_id)
        return ctx

    def pseudonym(self, session_id: str) -> str:
        """Stable per-value placeholder: equal inputs map to equal outputs, so
        "all init events share one session" stays checkable without publishing
        the real identifier."""
        if session_id not in self.session_pseudonyms:
            self.session_pseudonyms[session_id] = f"<session-{len(self.session_pseudonyms) + 1:02d}>"
        return self.session_pseudonyms[session_id]

    def scrub_text(self, text: str) -> str:
        for session_id, alias in self.session_pseudonyms.items():
            text = text.replace(session_id, alias)
        for home in self.home_paths:
            text = text.replace(home, "~")
        if self.username:
            text = text.replace(self.username, "<user>")
        for name, pattern in _TOKEN_PATTERNS[:3]:
            text = pattern.sub(f"<redacted:{name}>", text)
        return text


def collect_session_ids(events: Iterable[Any]) -> list[str]:
    """Every ``session_id`` value anywhere in the capture, first-seen order."""
    found: list[str] = []

    def walk(node: Any) -> None:
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "session_id" and isinstance(value, str) and value not in found:
                    found.append(value)
                walk(value)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    for event in events:
        walk(event)
    return found


def redact_event(node: Any, ctx: RedactionContext) -> Any:
    """Structural redaction: walks the PARSED object, never the raw text."""
    if isinstance(node, dict):
        return {key: _redact_value(key, value, ctx) for key, value in node.items()}
    if isinstance(node, list):
        return [redact_event(item, ctx) for item in node]
    if isinstance(node, str):
        return ctx.scrub_text(node)
    return node


def _redact_value(key: str, value: Any, ctx: RedactionContext) -> Any:
    if isinstance(value, str):
        if key == "session_id":
            return ctx.pseudonym(value)
        if key in ("output_file", "cwd") and value:
            return f"<{key}>/{os.path.basename(value)}"
        if CREDENTIAL_NAME_PATTERN.search(key):
            return f"<redacted:{key}>"
    return redact_event(value, ctx)


def scan_for_secrets(text: str, session_ids: Iterable[str] = ()) -> list[str]:
    """Return the NAMES of matching patterns. Never returns a matched value."""
    matched: list[str] = []
    home = os.path.expanduser("~")
    username = os.path.basename(home)
    for candidate in {home, os.path.realpath(home)}:
        if candidate in text:
            matched.append("home_path")
            break
    if username and username in text:
        matched.append("os_username")
    if any(session_id in text for session_id in session_ids if session_id):
        matched.append("session_identifier")
    for name, pattern in _TOKEN_PATTERNS:
        if pattern.search(text):
            matched.append(name)
    return matched


def _atomic_publish(text: str, dest_path: str) -> None:
    """Write via a sibling temp file + ``os.replace`` so an interrupted publish
    cannot leave a half-sanitised file behind."""
    os.makedirs(os.path.dirname(dest_path), exist_ok=True)
    handle, tmp_path = tempfile.mkstemp(dir=os.path.dirname(dest_path), prefix=".publish-")
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as out:
            out.write(text)
        os.replace(tmp_path, dest_path)
    except BaseException:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)
        raise


def publish_jsonl(staged_path: str, dest_path: str, ctx: Optional[RedactionContext] = None) -> dict[str, Any]:
    """validate → structurally redact → secret-scan → atomic replace.

    Raises :class:`SecretScanError` — publishing nothing — if the sanitised
    text still matches. Reports which pattern matched, never the value.
    """
    with open(staged_path, encoding="utf-8", errors="replace") as handle:
        raw_lines = [line for line in handle.read().split("\n") if line.strip()]

    parsed, unparseable = [], 0
    for line in raw_lines:
        try:
            parsed.append(json.loads(line))
        except json.JSONDecodeError:
            unparseable += 1

    session_ids = collect_session_ids(parsed)
    ctx = ctx or RedactionContext.build(session_ids)
    for session_id in session_ids:
        ctx.pseudonym(session_id)

    body = "".join(json.dumps(redact_event(event, ctx)) + "\n" for event in parsed)
    matches = scan_for_secrets(body, session_ids)
    if matches:
        raise SecretScanError(
            f"refusing to publish {dest_path!r}: secret scan matched {sorted(set(matches))}"
        )
    _atomic_publish(body, dest_path)
    return {
        "dest": dest_path,
        "lines_staged": len(raw_lines),
        "lines_published": len(parsed),
        "unparseable_lines": unparseable,
        "session_pseudonyms": dict(ctx.session_pseudonyms),
    }


def publish_text(staged_path_or_text: str, dest_path: str, ctx: RedactionContext, *, is_text: bool = False) -> dict[str, Any]:
    """Same gate for non-JSON artifacts (stderr log, run log).

    These have no object structure to walk, so the shared string scrubber is
    applied to the whole body; the secret scan is the gate either way.
    """
    if is_text:
        raw = staged_path_or_text
    elif os.path.exists(staged_path_or_text):
        with open(staged_path_or_text, encoding="utf-8", errors="replace") as handle:
            raw = handle.read()
    else:
        raw = ""
    body = ctx.scrub_text(raw)
    matches = scan_for_secrets(body, ctx.session_pseudonyms.keys())
    if matches:
        raise SecretScanError(
            f"refusing to publish {dest_path!r}: secret scan matched {sorted(set(matches))}"
        )
    _atomic_publish(body, dest_path)
    return {"dest": dest_path, "bytes": len(body)}


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------

PROMPT_TEMPLATE = """You are part of a controlled experiment. Follow these steps EXACTLY.

STEP 1: Use your background-subagent-spawning tool (Agent or Task) to spawn
TWO SEPARATE subagents, both running in the background (async, do not wait
for either):

  Subagent A instruction: "Wait 10 seconds using the Bash tool (sleep 10),
  then write the exact text CHILD-A-DONE (no other text) to the file
  {signal_a} using the Bash tool, then stop."

  Subagent B instruction: "Wait 22 seconds using the Bash tool (sleep 22),
  then write the exact text CHILD-B-DONE (no other text) to the file
  {signal_b} using the Bash tool, then stop."

STEP 2: Immediately after spawning BOTH — same turn, no waiting, no
polling, no other tool calls — output exactly this text and nothing else:
ORCHESTRATOR-TURN-1-COMPLETE

End your turn immediately after that text.
"""


def resolve_cli_version() -> str:
    proc = subprocess.run(["claude", "--version"], capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise HarnessError(f"`claude --version` exited {proc.returncode}")
    return proc.stdout.strip().split()[0]


def _run_log_lines(run: LaunchedRun, obs: Observations, version: str, stage_dir: str) -> list[str]:
    residue = run.residue
    lines = [
        "# 30c run log — monitor-environment replication harness",
        f"claude_cli_version: {version}",
        f"expected_cli_version: {EXPECTED_CLI_VERSION}",
        f"workdir: {run.workdir}",
        f"staged_output_dir: {stage_dir}",
        f"staged_dir_outside_repo: {not os.path.realpath(stage_dir).startswith(os.path.realpath(REPO_ROOT) + os.sep)}",
        f"argv: {' '.join(run.argv)}",
        "",
        "## Replicated from spawn_monitor",
        "launch_via_sh_c: true",
        "detached_start_new_session: true",
        "stdin_tty: false / stdout_tty: false / stderr_tty: false",
        "stderr_separate_file: true",
        "",
        "## Deliberate deviation (the variable under test)",
        "harness_holds_child_stdin_open: true",
        "",
        "## Environment scrub (names only)",
        f"trial: {'2 — agent-session markers ALSO scrubbed' if run.scrubbed_markers else '1 — production git scrub only'}",
        f"scrub_list_source: crates/devflow-core/src/git.rs ({len(run.scrubbed_vars)} of the parsed names were present and removed)",
        f"removed_variables: {', '.join(run.scrubbed_vars) or '(none were set)'}",
        f"removed_agent_session_markers: {', '.join(run.scrubbed_markers) or '(none — trial 1 leaves these in place)'}",
        "",
        "## Residual environment the scrub cannot remove (names only, never values)",
        f"parent_process: {residue['parent_process']}",
        f"launched_from_agent_session: {residue['launched_from_agent_session']}",
        f"claude_session_markers: {', '.join(residue['claude_session_markers']) or '(none)'}",
        f"credential_named_var_count: {residue['credential_named_var_count']} (names withheld per T-30-07)",
        f"total_env_vars_in_child: {residue['total_env_vars']}",
        "",
        "## Timeline (seconds from launch)",
    ]
    lines += [f"{label}: t+{at:.2f}" for label, at in obs.timeline]
    lines += [f"signal_{label}: {value}" for label, value in sorted(obs.signals.items())]
    lines += [f"signal_{label}_contents: {value!r}" for label, value in sorted(obs.signal_contents.items())]
    lines += [
        f"wall_clock_duration_s: {obs.duration_s:.2f}" if obs.duration_s is not None else "wall_clock_duration_s: (not recorded)",
        f"exit_delay_after_stdin_close: {obs.exit_delay_after_stdin_close}",
        f"process_killed_by_harness: {obs.killed}",
        f"sh_exit_code: {obs.exit_code}",
        "",
        "## Observed result events (origin verbatim)",
    ]
    for index, result in enumerate(obs.results, 1):
        lines.append(
            f"result[{index}]: at=t+{result['at']:.2f} origin={json.dumps(result['origin'])} "
            f"num_turns={result['num_turns']} is_error={result['is_error']}"
        )
    lines += ["", "## Observed task notifications"]
    lines += [f"notification: at=t+{n['at']:.2f} task_id={n['task_id']} status={n['status']}" for n in obs.notifications]
    lines += ["", "## Observed background_tasks_changed"]
    lines += [f"background_tasks_changed: at=t+{c['at']:.2f} count={c['count']} ids={c['task_ids']}" for c in obs.background_task_changes]
    lines += ["", "# No verdict is printed here by design: 30a records that the", "# harness-printed verdict was wrong in v1 and unreliable in v2."]
    return lines


def run_single_trial(
    evidence_dir: str,
    version: str,
    *,
    scrub_markers: bool = False,
    expected_results: int = 3,
    read_deadline: float = 60.0,
    straggler_window: float = 5.0,
) -> dict[str, Any]:
    """Launch, observe and publish exactly one trial.

    Returns a summary of what was observed. It is deliberately NOT a verdict:
    the verdict is derived later by re-reading the published capture.
    """
    markers = discover_agent_session_markers() if scrub_markers else []
    stage_dir = make_stage_dir()
    signal_paths = {
        "A": os.path.join(stage_dir, "signalA_30c.txt"),
        "B": os.path.join(stage_dir, "signalB_30c.txt"),
    }
    prompt = PROMPT_TEMPLATE.format(signal_a=signal_paths["A"], signal_b=signal_paths["B"])

    print(f"[harness] staging raw output in {stage_dir}", flush=True)
    started = time.time()
    run = launch_in_monitor_env(REPO_ROOT, prompt, stage_dir=stage_dir, extra_scrub_names=markers)
    print(
        f"[harness] launched; scrubbed {len(run.scrubbed_vars)} git vars + "
        f"{len(run.scrubbed_markers)} agent markers; prompt sent, stdin HELD OPEN",
        flush=True,
    )

    obs = observe_run(
        run,
        signal_paths,
        expected_results=expected_results,
        read_deadline=read_deadline,
        straggler_window=straggler_window,
    )
    obs.duration_s = time.time() - started

    for label, path in signal_paths.items():
        content = open(path, encoding="utf-8").read().strip() if os.path.exists(path) else None
        obs.signal_contents[label] = content
        print(f"[harness] signal {label}: exists={os.path.exists(path)} content={content!r}", flush=True)

    ctx = RedactionContext.build()
    published = [
        publish_jsonl(run.stdout_path, os.path.join(evidence_dir, "raw_output.jsonl"), ctx),
        publish_text(run.stderr_path, os.path.join(evidence_dir, "stderr.log"), ctx),
    ]
    log_body = "\n".join(_run_log_lines(run, obs, version, stage_dir)) + "\n"
    published.append(publish_text(log_body, os.path.join(evidence_dir, "run.log"), ctx, is_text=True))

    print("\n[harness] published (sanitised) —")
    for entry in published:
        print(f"  {entry}", flush=True)
    print(f"[harness] staged raw output retained at {stage_dir} (outside .planning/)", flush=True)
    print("[harness] observations only; the verdict is derived from the PUBLISHED file.", flush=True)
    return {
        "evidence_dir": evidence_dir,
        "stage_dir": stage_dir,
        "duration_s": obs.duration_s,
        "git_vars_removed": len(run.scrubbed_vars),
        "markers_removed": len(run.scrubbed_markers),
        "result_events": len(obs.results),
        "signal_contents": dict(obs.signal_contents),
        "published": published,
    }


def _run_replication_set(version: str, count: int) -> int:
    """`count` trials in the trial-2 scrubbed configuration, held fixed.

    Two things are deliberate here. The git decoys are planted so production's
    scrub does real work during each measured run. And every trial gets an
    IDENTICAL observation window — `expected_results` is set beyond reach so no
    trial early-stops. A variable window would confound a replication set,
    because a trial that stopped early could look like it delivered fewer
    events than one that ran longer.
    """
    os.environ.update(RELIABILITY_DECOY_GIT_ENV)
    print(f"[harness] planted git decoys: {sorted(RELIABILITY_DECOY_GIT_ENV)}", flush=True)
    summaries = []
    for index in range(1, count + 1):
        print(f"\n{'=' * 70}\n[harness] replication trial {index}/{count}\n{'=' * 70}", flush=True)
        summary = run_single_trial(
            os.path.join(EVIDENCE_DIR_RELIABILITY, f"trial-{index}"),
            version,
            scrub_markers=True,
            expected_results=10**6,  # never trips: fixed observation window
            read_deadline=75.0,
        )
        summary["trial"] = index
        summaries.append(summary)
        print(
            f"[harness] trial {index} done in {summary['duration_s']:.1f}s; "
            f"git vars removed={summary['git_vars_removed']} "
            f"markers removed={summary['markers_removed']} "
            f"result events={summary['result_events']} "
            f"signals={summary['signal_contents']}",
            flush=True,
        )
    print(f"\n[harness] {count} trials complete. Derive verdicts from the published captures.", flush=True)
    return 0


def _int_flag(argv: list[str], name: str) -> Optional[int]:
    if name not in argv:
        return None
    position = argv.index(name)
    if position + 1 >= len(argv):
        raise HarnessError(f"{name} requires a count")
    return int(argv[position + 1])


def main(argv: Optional[list[str]] = None) -> int:
    argv = sys.argv[1:] if argv is None else argv

    version = resolve_cli_version()
    if version != EXPECTED_CLI_VERSION:
        raise HarnessError(
            f"claude CLI is {version!r}, expected {EXPECTED_CLI_VERSION!r} — the "
            "version every archived 30a capture recorded. A different CLI makes "
            "comparison against the baseline invalid."
        )

    replicate = _int_flag(argv, "--replicate")
    if replicate:
        return _run_replication_set(version, replicate)

    scrub_markers = "--scrub-agent-markers" in argv
    print(f"[harness] trial {'2 (agent-session markers scrubbed)' if scrub_markers else '1 (production git scrub only)'}", flush=True)
    run_single_trial(
        EVIDENCE_DIR_SCRUBBED if scrub_markers else EVIDENCE_DIR,
        version,
        scrub_markers=scrub_markers,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
