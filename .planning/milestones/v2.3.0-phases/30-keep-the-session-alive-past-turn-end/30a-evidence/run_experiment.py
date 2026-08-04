#!/usr/bin/env python3
"""
999.64 feasibility experiment.

Question: under `claude -p --input-format stream-json` (session kept alive past
turn end until stdin closes), if the orchestrator backgrounds a subagent and
ends its turn WITHOUT waiting, does the subagent's completion notification
actually get delivered into the still-open session, and does the orchestrator
resume and act on it?

This is deliberately isolated: a scratch git repo, no GSD, no worktrees, no
devflow-core state.
"""
import json
import os
import subprocess
import sys
import time

WORKDIR = os.path.dirname(os.path.abspath(__file__))
SIGNAL_FILE = os.path.join(WORKDIR, "signal.txt")
LOG_FILE = os.path.join(WORKDIR, "raw_output.jsonl")

if os.path.exists(SIGNAL_FILE):
    os.remove(SIGNAL_FILE)

PROMPT = f"""You are part of a controlled experiment. Follow these steps EXACTLY.

STEP 1: Use your background-subagent-spawning tool (Agent or Task, whichever is
available to you) to spawn EXACTLY ONE subagent, running in the background
(do not wait for it — background/async mode), with this exact instruction for
the subagent: "Wait 12 seconds using the Bash tool (sleep 12), then write the
exact text EXPERIMENT-DONE (no other text) to the file {SIGNAL_FILE} using the
Bash tool, then stop."

STEP 2: Immediately after spawning it in step 1 — in the SAME turn, without
waiting, polling, or checking on the subagent — output exactly this text and
nothing else: ORCHESTRATOR-TURN-1-COMPLETE

Do not call any other tools. Do not wait for the subagent. End your turn
immediately after step 2's text.
"""

req = {
    "type": "user",
    "message": {"role": "user", "content": PROMPT},
}

cmd = [
    "claude", "-p",
    "--input-format", "stream-json",
    "--output-format", "stream-json",
    "--verbose",
    "--dangerously-skip-permissions",
]

print(f"[harness] workdir={WORKDIR}")
print(f"[harness] signal_file={SIGNAL_FILE}")
print(f"[harness] launching: {' '.join(cmd)}")

t0 = time.time()
proc = subprocess.Popen(
    cmd, cwd=WORKDIR,
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    text=True, bufsize=1,
)

proc.stdin.write(json.dumps(req) + "\n")
proc.stdin.flush()
print(f"[harness] t+{time.time()-t0:.1f} initial prompt sent, stdin HELD OPEN")

events = []
log = open(LOG_FILE, "w")

turn1_seen_at = None
signal_seen_at = None
turn2_seen_at = None
turn2_text = None

DEADLINE = 70  # seconds

while time.time() - t0 < DEADLINE:
    line = proc.stdout.readline()
    if line == "" and proc.poll() is not None:
        print(f"[harness] t+{time.time()-t0:.1f} process exited (code={proc.returncode}) while stdin still open")
        break
    if not line:
        # non-blocking-ish: process alive, no output yet, check signal file, keep polling
        if signal_seen_at is None and os.path.exists(SIGNAL_FILE):
            signal_seen_at = time.time() - t0
            print(f"[harness] t+{signal_seen_at:.1f} SIGNAL FILE WRITTEN (subagent completed real work)")
        time.sleep(0.3)
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
    events.append(ev)

    et = ev.get("type")
    if et == "assistant":
        content = ev.get("message", {}).get("content", [])
        text = "".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")
        if text.strip():
            elapsed = time.time() - t0
            print(f"[harness] t+{elapsed:.1f} assistant text: {text.strip()[:120]!r}")
            if "ORCHESTRATOR-TURN-1-COMPLETE" in text and turn1_seen_at is None:
                turn1_seen_at = elapsed
                print(f"[harness] t+{elapsed:.1f} >>> TURN 1 marker seen (orchestrator declared done, backgrounded child)")
            elif turn1_seen_at is not None and turn2_seen_at is None:
                turn2_seen_at = elapsed
                turn2_text = text.strip()
                print(f"[harness] t+{elapsed:.1f} >>> SECOND ASSISTANT TURN after turn1 — orchestrator RESUMED")
    elif et == "system":
        print(f"[harness] t+{time.time()-t0:.1f} system/{ev.get('subtype')}")
    elif et == "result":
        print(f"[harness] t+{time.time()-t0:.1f} result: subtype={ev.get('subtype')} is_error={ev.get('is_error')}")

    if signal_seen_at is None and os.path.exists(SIGNAL_FILE):
        signal_seen_at = time.time() - t0
        print(f"[harness] t+{signal_seen_at:.1f} SIGNAL FILE WRITTEN (subagent completed real work)")

    # stop condition: we've seen turn1, then either a second turn or the signal + enough wait
    if turn1_seen_at is not None and turn2_seen_at is not None:
        print(f"[harness] both markers observed, waiting 5s more for trailing events then closing stdin")
        time.sleep(5)
        break

print(f"[harness] t+{time.time()-t0:.1f} closing stdin")
try:
    proc.stdin.close()
except Exception:
    pass

try:
    proc.wait(timeout=15)
except subprocess.TimeoutExpired:
    print(f"[harness] process did not exit within 15s of stdin close — killing")
    proc.kill()
    proc.wait()

print(f"[harness] t+{time.time()-t0:.1f} process exited, code={proc.returncode}")
log.close()

print()
print("=" * 70)
print("RESULT")
print("=" * 70)
print(f"turn1_seen_at         = {turn1_seen_at}")
print(f"signal_file_written   = {signal_seen_at}")
print(f"turn2_seen_at         = {turn2_seen_at}")
print(f"turn2_text            = {turn2_text!r}")
print(f"signal_file_exists    = {os.path.exists(SIGNAL_FILE)}")
if os.path.exists(SIGNAL_FILE):
    print(f"signal_file_content   = {open(SIGNAL_FILE).read()!r}")

verdict = "UNKNOWN"
if turn1_seen_at is not None and turn2_seen_at is not None and signal_seen_at is not None:
    if turn2_seen_at > signal_seen_at:
        verdict = "CONFIRMED: session survived turn end, subagent completed, and a SECOND orchestrator turn followed after the child's completion — delivery into the live session is real."
    else:
        verdict = "AMBIGUOUS: second turn occurred before the signal file was observed by the harness — check raw log for ordering, may be a polling artifact."
elif turn1_seen_at is not None and signal_seen_at is not None and turn2_seen_at is None:
    verdict = "REFUTED: subagent completed real work (signal file written) but NO second orchestrator turn followed — completion was NOT delivered into the live session, or was delivered but not acted on."
elif turn1_seen_at is None:
    verdict = "INCONCLUSIVE: orchestrator never reached turn-1 marker — prompt or tool-availability issue, not a test of the property."
else:
    verdict = f"INCONCLUSIVE: turn1={turn1_seen_at} signal={signal_seen_at} turn2={turn2_seen_at}"

print()
print(f"VERDICT: {verdict}")
