#!/usr/bin/env python3
"""
999.64 feasibility experiment, v3.

Combines the three follow-ups the v2 result called for:
1. A repeat trial (reduces flukiness risk) -- satisfied by observing TWO
   independent notification-delivery events in one run, not by rerunning
   the identical single-child case.
2. TWO CONCURRENT background subagents, matching Phase 29 wave-2's actual
   shape (that wave had 2 plans dispatched in background simultaneously),
   not v2's single trivial child.
3. A clean exit-timing measurement after stdin close -- v2 never got this
   because the harness hung on a blocking readline() with no timeout.

Fix applied: use select() with a timeout on every read, so the read loop
can never block indefinitely regardless of what the child process does.
"""
import json
import os
import select
import subprocess
import time

WORKDIR = os.path.dirname(os.path.abspath(__file__))
SIGNAL_A = os.path.join(WORKDIR, "signalA_v3.txt")
SIGNAL_B = os.path.join(WORKDIR, "signalB_v3.txt")
LOG_FILE = os.path.join(WORKDIR, "raw_output_v3.jsonl")

for f in (SIGNAL_A, SIGNAL_B):
    if os.path.exists(f):
        os.remove(f)

PROMPT = f"""You are part of a controlled experiment. Follow these steps EXACTLY.

STEP 1: Use your background-subagent-spawning tool (Agent or Task) to spawn
TWO SEPARATE subagents, both running in the background (async, do not wait
for either):

  Subagent A instruction: "Wait 10 seconds using the Bash tool (sleep 10),
  then write the exact text CHILD-A-DONE (no other text) to the file
  {SIGNAL_A} using the Bash tool, then stop."

  Subagent B instruction: "Wait 22 seconds using the Bash tool (sleep 22),
  then write the exact text CHILD-B-DONE (no other text) to the file
  {SIGNAL_B} using the Bash tool, then stop."

STEP 2: Immediately after spawning BOTH — same turn, no waiting, no
polling, no other tool calls — output exactly this text and nothing else:
ORCHESTRATOR-TURN-1-COMPLETE

End your turn immediately after that text.
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
result_events = []          # (t, num_turns, origin)
top_level_after_each_result = {}
signal_a_at = None
signal_b_at = None
last_activity = time.time()

READ_DEADLINE = 45           # 22s child + generous buffer for notification delivery
IDLE_STOP_AFTER = 6          # stop polling for new events once quiet this long past deadline trigger conditions met

def check_signals():
    global signal_a_at, signal_b_at
    now = time.time() - t0
    if signal_a_at is None and os.path.exists(SIGNAL_A):
        signal_a_at = now
        print(f"[harness] t+{now:.1f} *** SIGNAL A WRITTEN (child A completed) ***", flush=True)
    if signal_b_at is None and os.path.exists(SIGNAL_B):
        signal_b_at = now
        print(f"[harness] t+{now:.1f} *** SIGNAL B WRITTEN (child B completed) ***", flush=True)

while time.time() - t0 < READ_DEADLINE:
    check_signals()
    ready, _, _ = select.select([proc.stdout], [], [], 0.5)
    if not ready:
        if proc.poll() is not None:
            print(f"[harness] t+{time.time()-t0:.1f} process exited on its own (code={proc.returncode})", flush=True)
            break
        continue

    line = proc.stdout.readline()
    if line == "":
        if proc.poll() is not None:
            print(f"[harness] t+{time.time()-t0:.1f} process exited (EOF, code={proc.returncode})", flush=True)
            break
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
    tag = "TOP" if parent is None else "sub"

    if et == "result":
        origin = ev.get("origin", {}).get("kind")
        num_turns = ev.get("num_turns")
        result_events.append((elapsed, num_turns, origin))
        print(f"[harness] t+{elapsed:.1f} *** RESULT #{len(result_events)} num_turns={num_turns} origin={origin} ***", flush=True)

    desc = ""
    if et == "assistant":
        content = ev.get("message", {}).get("content", [])
        texts = [b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text"]
        tools = [b.get("name") for b in content if isinstance(b, dict) and b.get("type") == "tool_use"]
        desc = f"text={texts!r} tools={tools!r}"
    elif et == "system":
        desc = f"subtype={ev.get('subtype')}"
        if ev.get("subtype") == "task_notification":
            desc += f" status={ev.get('status')} summary={str(ev.get('summary'))[:60]!r}"
    print(f"[harness] t+{elapsed:.1f} [{tag}] {et}: {desc}", flush=True)

    check_signals()

    # Stop once we've seen 3 results (turn1, childA-ack, childB-ack) and both signals written
    if len(result_events) >= 3 and signal_a_at is not None and signal_b_at is not None:
        print(f"[harness] t+{elapsed:.1f} both children confirmed complete and acknowledged, waiting 3s for stragglers", flush=True)
        stop_at = time.time() + 3
        while time.time() < stop_at:
            r, _, _ = select.select([proc.stdout], [], [], 0.3)
            if r:
                l2 = proc.stdout.readline()
                if l2:
                    log.write(l2)
            check_signals()
        break

print(f"[harness] t+{time.time()-t0:.1f} closing stdin now", flush=True)
close_t = time.time()
try:
    proc.stdin.close()
except Exception:
    pass

exit_delay = None
deadline2 = time.time() + 20
while time.time() < deadline2:
    if proc.poll() is not None:
        exit_delay = time.time() - close_t
        break
    r, _, _ = select.select([proc.stdout], [], [], 0.5)
    if r:
        l3 = proc.stdout.readline()
        if l3:
            log.write(l3)
    time.sleep(0.1)

if exit_delay is None:
    print(f"[harness] process did NOT exit within 20s of stdin close -- killing", flush=True)
    proc.kill()
    proc.wait()
else:
    print(f"[harness] process exited {exit_delay:.2f}s after stdin close, code={proc.returncode}", flush=True)

log.close()

print()
print("=" * 70)
print("RESULT (v3)")
print("=" * 70)
for i, (t, nt, origin) in enumerate(result_events):
    print(f"result #{i+1}: t+{t:.1f} num_turns={nt} origin={origin}")
print(f"signal_a_at (child A done) = {signal_a_at}")
print(f"signal_b_at (child B done) = {signal_b_at}")
print(f"exit_delay_after_stdin_close = {exit_delay}")
print(f"signal_a_exists = {os.path.exists(SIGNAL_A)}  content = {open(SIGNAL_A).read() if os.path.exists(SIGNAL_A) else None!r}")
print(f"signal_b_exists = {os.path.exists(SIGNAL_B)}  content = {open(SIGNAL_B).read() if os.path.exists(SIGNAL_B) else None!r}")

notif_results = [r for r in result_events if r[2] == "task-notification"]
print()
if len(notif_results) >= 2:
    verdict = f"CONFIRMED (n=2 in one run): {len(notif_results)} separate task-notification-origin results observed -- BOTH concurrent children's completions were independently delivered into the live session and each triggered a real orchestrator turn."
elif len(notif_results) == 1:
    verdict = "PARTIAL: only ONE of two concurrent children's completions produced a task-notification result -- the other's delivery is unconfirmed or merged/lost. Needs investigation, not assumed benign."
else:
    verdict = "REFUTED for the concurrent case: no task-notification-origin result observed despite child(ren) completing."
print(f"VERDICT: {verdict}")
if exit_delay is not None:
    print(f"EXIT TIMING: process terminated {exit_delay:.2f}s after stdin close -- {'prompt' if exit_delay < 5 else 'SLOW, investigate'}")
else:
    print("EXIT TIMING: process did not exit on its own within 20s of stdin close -- required a kill. This is a real finding for 30b's design, not a harness artifact this time (select()-based read, no blocking).")
