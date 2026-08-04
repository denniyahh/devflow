# Phase 30 — Deferred Items (out-of-scope discoveries)

Issues discovered during execution that are unrelated to the current task's
changes and therefore not auto-fixed, per the executor's scope-boundary rule.

## 1. The committed `30a-evidence/raw_output_v3.jsonl` carries an unredacted home path, OS username and session id

**Found during:** 30-02 Task 1, while proving the new publish pipeline against a
real capture rather than only a synthetic fixture.

**Issue:** Running 30c's own secret scanner over the already-committed baseline
capture matches three patterns:

```
scan_for_secrets(30a-evidence/raw_output_v3.jsonl)
  -> ['home_path', 'os_username', 'session_identifier']
```

This is the live instance the cross-AI review cited as the reason 30-02's
original single-step evidence write was unsafe. Concretely, the `init` events
(lines 5, 32, 47) carry an absolute `cwd` under the operator's home directory,
every line carries the same real `session_id`
(`559f…`, one distinct value across all 54 lines), and the `task_notification`
events at lines 31 and 46 carry absolute `output_file` paths. **All three**
archived captures were scanned and all three match the same three patterns:

| Capture | Lines | Committed scan |
|---------|-------|----------------|
| `raw_output.jsonl` | 12 | `home_path`, `os_username`, `session_identifier` |
| `raw_output_v2.jsonl` | 25 | `home_path`, `os_username`, `session_identifier` |
| `raw_output_v3.jsonl` | 54 | `home_path`, `os_username`, `session_identifier` |

**Why not fixed here:** 30-02's `files_modified` lists only the three 30c paths,
and the plan carries an explicit scope fence. Rewriting a sibling unit's
committed evidence would also invalidate the line-number citations that
30-01-PLAN.md, 30-02-PLAN.md, 30-01-SUMMARY.md and `30-REVIEWS.md` all make into
that exact file — a change with real blast radius that should be taken
deliberately, not as a drive-by during an unrelated task.

**Severity:** low-but-real. The operator's GitHub username is already public in
this repository's commit metadata, and the session id is a local identifier with
no credential value. Nothing credential-shaped matched. It is nonetheless the
same leak class as backlog 999.10 and Phase 18 review finding WR-02.

**Fix available at near-zero cost:** the pipeline that sanitised 30c's evidence
is already written, already proven against this exact file, and importable:

```python
publish_jsonl("<staged-or-existing>.jsonl", "<dest>.jsonl")   # 30c-monitor-env-harness.py
```

All three 30a captures were re-published through it into a scratch directory
during verification, and all three came back `CLEAN` with `unparseable=0` and no
line loss (12/12, 25/25, 54/54). Doing it for real needs a companion pass over
the line-number citations in the four documents above.

**Not appended to `.planning/WINDOWS.md`:** an open ledger entry blocks
`/gsd-ship`, and blocking this phase's ship on a pre-existing artifact the phase
deliberately scoped out is a policy call for the operator, not this executor.
Escalate if the ledger is the preferred home. (30-01's executor declined the
same call for the same reason on finding F-1.)
