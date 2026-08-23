#!/usr/bin/env python3
"""
999.64 feasibility experiment, v2 — corrected after v1's flawed turn-detection
heuristic mistook the subagent's own forwarded narration (parent_tool_use_id
set) for orchestrator resumption. This version distinguishes top-level
orchestrator events (parent_tool_use_id is null/absent) from subagent-
forwarded events (parent_tool_use_id present), and does not stop reading
based on any text heuristic -- it reads continuously past the child's
expected completion time.

Question: after the child subagent (spawned in background, ~12s of work)
completes, does a NEW top-level (orchestrator) event appear in the still-open
stream, proving the completion was delivered into the live session -- not
just that the child's own progress narration streams through?
"""
import json
import os
import subprocess
import time

WORKDIR = os.path.dirname(os.path.abspath(__file__))
SIGNAL_FILE = os.path.join(WORKDIR, "signal2.txt")
LOG_FILE = os.path.join(WORKDIR, "raw_output_v2.jsonl")

if os.path.exists(SIGNAL_FILE):
    os.remove(SIGNAL_FILE)

PROMPT = f"""You are part of a controlled experiment. Follow these steps EXACTLY.

STEP 1: Use your background-subagent-spawning tool (Agent or Task) to spawn
EXACTLY ONE subagent, running in the background (async, do not wait), with
this exact instruction for the subagent: "Wait 15 seconds using the Bash tool
(sleep 15), then write the exact text EXPERIMENT-DONE-V2 (no other text) to
the file {SIGNAL_FILE} using the Bash tool, then stop."

STEP 2: Immediately after spawning it — same turn, no waiting, no polling,
no other tool calls — output exactly this text and nothing else:
ORCHESTRATOR-TURN-1-COMPLETE

End your turn immediately after that text. Do not call any other tools.
"""

req = {"type": "user", "message": {"role": "user", "content": PROMPT}}

cmd = [
    "claude", "-p",
    "--input-format", "stream-json",
    "--output-format", "stream-json",
    "--verbose",
    "--dangerously-skip-permissions",
]

print(f"[harness] workdir={WORKDIR}", flush=True)
print(f"[harness] signal_file={SIGNAL_FILE}", flush=True)

t0 = time.time()
proc = subprocess.Popen(
    cmd, cwd=WORKDIR,
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    text=True, bufsize=1,
)
proc.stdin.write(json.dumps(req) + "\n")
proc.stdin.flush()
print(f"[harness] t+{time.time()-t0:.1f} prompt sent, stdin HELD OPEN", flush=True)

log = open(LOG_FILE, "w")
orch_session_id = None
top_level_events = []       # (t, type, subtype_or_text)
subagent_events = []        # (t, type, subtype_or_text)
task_started_at = None
result_seen_at = None
signal_seen_at = None
first_top_level_after_result = None
first_top_level_after_signal = None

READ_DEADLINE = 45  # comfortably past the child's 15s dispatched-at-~t+6-8 => expect completion ~t+21-23

while time.time() - t0 < READ_DEADLINE:
    line = proc.stdout.readline()
    if line == "" and proc.poll() is not None:
        print(f"[harness] t+{time.time()-t0:.1f} process exited on its own (code={proc.returncode})", flush=True)
        break
    if not line:
        if signal_seen_at is None and os.path.exists(SIGNAL_FILE):
            signal_seen_at = time.time() - t0
            print(f"[harness] t+{signal_seen_at:.1f} *** SIGNAL FILE WRITTEN (child completed real work) ***", flush=True)
        time.sleep(0.2)
        continue

    log.write(line)
    log.flush()
    line = line.strip()
    if not line:
        continue
    try:
        ev = json.loads(line)
    except json.JSONDecodeError:
        continue

    elapsed = time.time() - t0
    et = ev.get("type")
    parent = ev.get("parent_tool_use_id")
    is_top_level = parent is None

    if et == "system" and ev.get("subtype") == "init":
        orch_session_id = ev.get("session_id")

    if et == "system" and ev.get("subtype") == "task_started" and task_started_at is None:
        task_started_at = elapsed
        print(f"[harness] t+{elapsed:.1f} task_started (child dispatched)", flush=True)

    if et == "result":
        result_seen_at = elapsed
        print(f"[harness] t+{elapsed:.1f} *** RESULT event (subtype={ev.get('subtype')}) -- this is where a plain `claude -p` process would normally EXIT ***", flush=True)

    if signal_seen_at is None and os.path.exists(SIGNAL_FILE):
        signal_seen_at = time.time() - t0
        print(f"[harness] t+{signal_seen_at:.1f} *** SIGNAL FILE WRITTEN (child completed real work) ***", flush=True)

    desc = None
    if et == "assistant":
        content = ev.get("message", {}).get("content", [])
        texts = [b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text"]
        tools = [b.get("name") for b in content if isinstance(b, dict) and b.get("type") == "tool_use"]
        desc = f"text={texts!r} tools={tools!r}"
    elif et == "system":
        desc = f"subtype={ev.get('subtype')}"
    elif et == "result":
        desc = f"subtype={ev.get('subtype')} is_error={ev.get('is_error')}"
    elif et == "user":
        desc = "tool_result-or-user-msg"
    else:
        desc = str(ev)[:80]

    tag = "TOP-LEVEL" if is_top_level else f"subagent(parent={parent[:12]}...)"
    print(f"[harness] t+{elapsed:.1f} [{tag}] {et}: {desc}", flush=True)

    if is_top_level:
        top_level_events.append((elapsed, et, desc))
        if result_seen_at is not None and elapsed > result_seen_at and first_top_level_after_result is None and et != "result":
            first_top_level_after_result = (elapsed, et, desc)
            print(f"[harness] t+{elapsed:.1f} >>>>> FIRST TOP-LEVEL EVENT AFTER RESULT <<<<<", flush=True)
        if signal_seen_at is not None and elapsed > signal_seen_at and first_top_level_after_signal is None:
            first_top_level_after_signal = (elapsed, et, desc)
            print(f"[harness] t+{elapsed:.1f} >>>>> FIRST TOP-LEVEL EVENT AFTER CHILD COMPLETION <<<<<", flush=True)
    else:
        subagent_events.append((elapsed, et, desc))

print(f"[harness] t+{time.time()-t0:.1f} read deadline reached or process ended, closing stdin", flush=True)
try:
    proc.stdin.close()
except Exception:
    pass

close_t = time.time()
try:
    proc.wait(timeout=20)
    exit_delay = time.time() - close_t
    print(f"[harness] process exited {exit_delay:.1f}s after stdin close, code={proc.returncode}", flush=True)
except subprocess.TimeoutExpired:
    print(f"[harness] process did NOT exit within 20s of stdin close -- killing", flush=True)
    proc.kill()
    proc.wait()
    exit_delay = None

log.close()

print()
print("=" * 70)
print("RESULT (v2)")
print("=" * 70)
print(f"orchestrator session_id       = {orch_session_id}")
print(f"task_started_at               = {task_started_at}")
print(f"result_seen_at                = {result_seen_at}")
print(f"signal_seen_at (child done)   = {signal_seen_at}")
print(f"first_top_level_after_result  = {first_top_level_after_result}")
print(f"first_top_level_after_signal  = {first_top_level_after_signal}")
print(f"total top-level events        = {len(top_level_events)}")
print(f"total subagent-tagged events  = {len(subagent_events)}")
print(f"process exit delay after stdin close = {exit_delay}")
print(f"signal_file_exists            = {os.path.exists(SIGNAL_FILE)}")

print()
if signal_seen_at is None:
    verdict = "INCONCLUSIVE: child never completed within the read window -- rerun with a longer deadline."
elif result_seen_at is not None and signal_seen_at > result_seen_at and first_top_level_after_signal is not None:
    verdict = ("CONFIRMED: the CLI's `result` event (normal exit point) fired BEFORE the child completed, "
               "the process stayed alive (stdin still open), the child then completed, and a NEW top-level "
               "orchestrator event followed. Delivery into the live session is real.")
elif result_seen_at is not None and signal_seen_at > result_seen_at and first_top_level_after_signal is None:
    verdict = ("REFUTED: `result` fired before the child completed, the child did complete (signal file written), "
               "but NO further top-level orchestrator event followed within the read window. The session stayed "
               "alive but nothing woke it -- completion was not delivered as an actionable event to the orchestrator.")
elif result_seen_at is None:
    verdict = ("DIFFERENT SHAPE THAN EXPECTED: no `result` event fired at all before the child completed -- the "
               "orchestrator's own turn did not conclude quickly. Re-examine raw log for what actually happened.")
else:
    verdict = f"UNCLASSIFIED: result_seen_at={result_seen_at} signal_seen_at={signal_seen_at}"

print(f"VERDICT: {verdict}")
