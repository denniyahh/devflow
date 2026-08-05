# Phase 34 — per-stage `stream-json` capture evidence

Landing spot for the capture campaign's per-stage evidence (999.73 criteria 1 and 2). Created
**empty and ahead of time** by plan 34-02, so the copy-at-landing step in plan 34-05 has somewhere
to put a capture instead of inventing a layout under time pressure.

**Nothing here is a capture yet.** There is nothing to capture until plan 34-05 widens
`STREAM_JSON_STAGES` and rebuilds the binary.

## Layout

One directory per stage, named for the stage:

```
34-evidence/
├── README.md      (this file)
├── define/
├── plan/
├── code/
├── validate/
└── ship/
```

Each stage directory holds, once its capture lands:

| File | Content |
|---|---|
| `raw_output.jsonl` | The capture copied **verbatim** out of `.devflow/`, unmodified except for the PII scrub below |
| `run.log` | Short human-readable provenance note, Phase 30 format (below) |
| `stderr.log` | The run's stderr, if any. Phase 30 kept this even at 0 bytes — an empty stderr is itself a finding |

## `run.log` — required fields

Reproduce Phase 30's format
(`.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30c-evidence/run.log`).
Each per-stage log must carry:

- **command invoked** — the full argv
- **stage** — which of the five this capture is from
- **`claude` CLI version** — `claude_cli_version:`, as Phase 30 recorded it
- **`devflow` build commit** — the binary the capture was produced by
- **git commit of `STREAM_JSON_STAGES`** at capture time — the constant's value is the variable
  the whole campaign turns on, so pin which revision produced this capture
- **outcome** — what the run did

## Committing a capture: copy out, never `git add -f`

`.devflow/.gitignore` is literally `*`. A capture written there is invisible to `git add` by
design.

**The correct route is to copy the file out of `.devflow/` into the stage directory here, then
commit the copy.** Do **not** reach for `git add -f` on the original `.devflow/` path. Force-adding
past the ignore rule commits the live working file rather than a frozen artifact, and it starts
tracking a path the tooling prunes and rewrites underneath git.

## PII scrub — three fields, before commit

Anything copied into `.planning/` becomes a permanent public artifact on push. Replace each of
these with a placeholder before committing, per Phase 30's redaction table:

- `home_path` → `<cwd>`
- `os_username` → placeholder
- `session_identifier` → `<session-01>`

## What Phase 30's raw / scrubbed / operator split actually was

Phase 30's three capture directories were **three separate runs, not one file redacted three
ways** — the byte counts differ (64939 / 55898 / 56165) precisely because each is an independent
run. "Scrubbed" meant agent-session environment markers (`CLAUDE_*` / `AI_AGENT*` / `ANTHROPIC*`)
removed **before launch**; "operator" meant a third trial run entirely outside any agent session.

**Phase 34 does not reproduce that three-way split.** Per D-10 this phase takes a single n=1 run
per stage, so each stage directory reproduces the **raw variant plus the PII scrub above** — one
capture, copied out, scrubbed. Do not label a post-hoc redaction of `raw_output.jsonl` as
"scrubbed"; in Phase 30's vocabulary that word means a different run, and reusing it for a
redacted copy would misrepresent what the evidence is.
