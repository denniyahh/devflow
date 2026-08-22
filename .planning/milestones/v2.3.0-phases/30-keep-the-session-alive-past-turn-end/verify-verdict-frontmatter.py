#!/usr/bin/env python3
"""Recompute every 30c verdict frontmatter count from the raw capture and diff them.

Usage:
    python3 verify-verdict-frontmatter.py <VERDICT.md> <raw_output.jsonl>

Prints CLAIMED vs MEASURED per field and exits non-zero on any mismatch, so the
check cannot pass by being skimmed. Deliberately recomputes from the JSONL rather
than trusting anything the harness printed — 30a records harness-printed verdicts
as wrong in v1 and unreliable in v2.
"""
import json
import sys


def frontmatter(path):
    """Parse the leading --- ... --- block into a flat dict of strings."""
    out, inside = {}, False
    for line in open(path, encoding="utf-8"):
        s = line.rstrip("\n")
        if s.strip() == "---":
            if inside:
                break
            inside = True
            continue
        if inside and ":" in s and not s.startswith((" ", "-")):
            k, v = s.split(":", 1)
            out[k.strip()] = v.strip()
    return out


def measure(path):
    events, unparseable = [], 0
    for line in open(path, encoding="utf-8"):
        if not line.strip():
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            unparseable += 1

    spawned = {
        t["task_id"]
        for e in events
        if e.get("subtype") == "background_tasks_changed"
        for t in (e.get("tasks") or [])
    }
    notified = {
        e.get("task_id")
        for e in events
        if e.get("subtype") == "task_notification"
    }
    delivered = spawned & notified
    results = [e for e in events if e.get("type") == "result"]

    return {
        "result_events": len(results),
        "task_notification_origin_results": sum(
            1 for r in results if (r.get("origin") or {}).get("kind") == "task-notification"
        ),
        "children_completed": len(delivered),
        "task_set_drained": any(
            e.get("subtype") == "background_tasks_changed" and e.get("tasks") == []
            for e in events
        ),
        "partial_delivery": bool(spawned) and delivered != spawned,
        "unparseable_lines": unparseable,
    }


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    claimed, measured = frontmatter(sys.argv[1]), measure(sys.argv[2])

    def norm(v):
        s = str(v).strip().lower()
        return {"true": "True", "false": "False"}.get(s, s)

    print(f"{'field':38} {'CLAIMED':>10} {'MEASURED':>10}   ")
    print("-" * 66)
    bad = 0
    for k, m in measured.items():
        c = claimed.get(k, "(absent)")
        ok = norm(c) == norm(m)
        bad += not ok
        print(f"{k:38} {str(c):>10} {str(m):>10}   {'ok' if ok else '<<< MISMATCH'}")
    print()
    print("ALL FIELDS MATCH" if not bad else f"{bad} MISMATCH(ES) — do not accept this verdict")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
