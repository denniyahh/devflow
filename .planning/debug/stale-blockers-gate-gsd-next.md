---
slug: stale-blockers-gate-gsd-next
status: diagnosed
trigger: "the two blockers reported in STATE.md — the RESOLVED 2026-07-19 (17-09, `cb9359f`) entry, and \"Phase 23 behavioral acceptance criterion (one phase Define-to-Ship, unattended) NOT met\""
created: 2026-08-03
updated: 2026-08-03
---

# Debug: stale blockers gate /gsd:next

## Symptoms

Symptoms were **prefilled by the orchestrator from direct verification**, not collected by
interview — the facts below were each checked against the live files in this session rather than
recalled. Treat them as evidence to re-test, not as conclusions to accept.

**Expected behavior.** With Phase 30 complete and Phase 31 created in ROADMAP.md, `/gsd:next`
should detect `situation: complete`/`needs-next-phase` and route to `/gsd:discuss-phase 31`.

**Actual behavior.** `gsd-tools smart-entry --json` returns:

```
situation: "blocked"
recommended: "debug"
summary: "Blocked · 2 blocker(s) — resolve before continuing"
```

and offers `/gsd:debug` as the recommended action. Forward motion to Phase 31 is gated.

**The two reported blockers, and what verification showed:**

1. `"**RESOLVED 2026-07-19 (17-09, \`cb9359f\`):**"` — this is the **bold header line of a
   RESOLVED entry**, not an open blocker. `.planning/STATE.md` line 709 begins that entry, and
   line 707 — the first line of the section — reads literally `None currently open for Phase 17.`
   Suspected mechanism: the blocker scan collects every `- ` bullet under `## Blockers` without
   honouring RESOLVED/CLOSED markers.

2. `"Phase 23 behavioral acceptance criterion (one phase Define-to-Ship, unattended) NOT met —
   23-11 acceptance run stopped at Define; requires Phase 23 merged to develop before Phase 24
   (or any new target) is reachable for a retry"` (STATE.md line 732). **Stale as written:**
   Phase 23 merged long ago, and Phases 24, 25, 26, 27, 28 and 30 have all since completed, so
   the stated precondition is satisfied. **But its underlying concern is live** — the criterion
   itself (one phase Define→Ship unattended) is still unmet; it is what Phase 25 was scoped
   around and is now owned by Phase 31's acceptance criterion (the live Phase 29 wave-2 re-run).
   Deleting the entry would drop a real concern; leaving it attributes that concern to a phase
   seven behind the current one.

**Error messages.** None. No command fails; `smart-entry` exits 0 and returns well-formed JSON.
The failure is a wrong verdict, not a crash — which is why nothing surfaces it.

**Timeline.** Unknown when it began. Both STATE.md entries predate this session (the RESOLVED one
is dated 2026-07-19; the Phase 23 one refers to a phase that closed well before Phase 30). It has
plausibly been reporting `blocked` for many phases without anyone acting on it.

**Reproduction.** Deterministic:

```bash
node ~/.claude/gsd-core/bin/gsd-tools.cjs smart-entry --json
# => situation: "blocked", signals.blockers: [2 entries above]
```

## Scope note for the investigator

Two candidate defects are in play and they are **not** the same thing. Please keep them separate
and do not let closing one imply the other:

- **Upstream (gsd-core):** the blocker scan counting RESOLVED entries as open. If confirmed, this
  is a false-positive safety gate — it makes `smart-entry` report `blocked` on a project with
  nothing blocked, and the recommended remedy (`/gsd:debug`) then chases a phantom.
- **Ours (this repository):** `## Blockers` in STATE.md is a mixed log — a "None currently open"
  line, a resolved entry retained for the record, and a stale item never cleaned up. Same class
  as backlog 999.72, where our document layout trips a GSD parser.

A related known-bad in the same tool, found while starting this session and worth noting rather
than chasing: `gsd-tools query state.load` emits JSON containing unescaped control characters in
`state_raw`, so `JSON.parse` rejects its output.

## Constraints

- **Do not clear or rewrite the `## Blockers` section without operator approval.** Clearing a
  blocker is a statement about project state and belongs to the operator. Diagnose freely;
  propose the edit; do not apply it unilaterally.
- Phase 31's ROADMAP entry and the `feature/phase-31` branch are already committed. Nothing here
  should modify them.

## Current Focus

hypothesis: CONFIRMED, and it is narrower than first stated. `smart-entry`'s blocker scan treats
  every `- ` bullet under `## Blockers` as an open blocker. It has no RESOLVED/CLOSED marker
  awareness and no "None currently open" sentinel awareness. But gsd-core's own shipped
  `templates/state.md` declares the contract the parser implements — "Keep only active blockers,
  remove resolved ones" — so the parser is CONSISTENT with its documented contract. The primary
  cause is therefore ours: STATE.md's `## Blockers` is used as an archive, against that contract.
test: read the predicate directly, then run the real tool against 7 fixtures where the Blockers
  section body is the ONLY variable, plus an as-is/cleared pair built from the real planning files.
expecting: RESOLVED-only must still report blocked (the bug); genuinely-open must still report
  blocked (negative control); sentinel-only and empty must not.
next_action: CHECKPOINT, BLOCKING — awaiting operator decision. The two remedies that change
  behaviour both fall under do-not-edit constraints (STATE.md Blockers section; gsd-core).
  The session manager could not collect the decision: AskUserQuestion is unavailable inside
  subagents, so the checkpoint was returned to the orchestrator unanswered. NOTHING was applied.

  ONE COUPLED DECISION, TWO PARTS — part 1 alone leaves the project recommending it close a
  milestone that still has Phase 31 outstanding:
    (1) Apply the proposed `## Blockers` replacement to STATE.md lines 705-733?
        (exact text at `proposed_state_md_blockers_replacement` below; verified on a copy:
        blockers 2 -> 0, negative control still reports a genuinely open blocker)
    (2) What to do about the `complete` / `new-milestone` verdict that appears once blockers
        are cleared? Options: (a) fix ROADMAP.md's missing `## Progress` table — already filed
        as backlog 999.72 / DEN-93, Size M; (b) accept `complete` as known-wrong and route to
        Phase 31 manually [investigator's recommendation]; (c) do neither and keep `blocked`,
        which is wrong but fails loudly.

  Third item, NOT asked and still open: upstream ledger entries 15 (blocker lifecycle) and 16
  (isComplete scope mismatch) are drafted in the Evidence section but unwritten — the ledger
  path `.planning/UPSTREAM-GSD-ISSUES.md` is a symlink into the gsd-core checkout that
  constraint 1 forbids editing, and is gitignored, so it is not a DevFlow-tracked file either.

  Specialist review (hint: typescript -> typescript-expert): NOT performed. The session manager
  has no Skill tool, and the only TypeScript change in play is the gsd-core fix that constraint
  1 forbids writing this session. Recorded as skipped-with-reason rather than fabricated.

reasoning_checkpoint:
  hypothesis: "Any line matching /^-\\s+(.+)$/ inside the `## Blockers` level-2 section is pushed
    into signals.blockers, and classify() returns 'blocked' when that array is non-empty. Our two
    bullets (a retained RESOLVED record and a stale Phase-23 item) are therefore counted as open."
  confirming_evidence:
    - "Read the predicate: smart-entry.cjs:285-287 + classify() at :401. No marker filter exists."
    - "Fixture (a), RESOLVED record only: observed situation=blocked, blockers=1."
    - "Fixture (d), verbatim copy of the live section: observed blocked, blockers=2 — byte-identical
       texts to the live run, so the reproduction is on a copy, not just the original."
  falsification_test: "A RESOLVED-only section yielding blockers=0 would have refuted it. Observed 1."
  fix_rationale: "The parser matches its own published contract (templates/state.md: 'Keep only
    active blockers, remove resolved ones'). So the defect that must be fixed to change the verdict
    is our document, not the tool. Removing the resolved record is what the contract asks for."
  blind_spots: "Not tested: whether any OTHER gsd workflow reads `## Blockers` as a historical log
    and would lose information if we trim it. state.cjs:1272-1278 duplicates the same open-set
    reading, which is weak evidence that nothing treats it as an archive, but I did not audit every
    consumer."
  candidate_causes:
    - "data: STATE.md `## Blockers` holds a resolved record and a stale item as live bullets"
    - "code: gsd-core's scan has no RESOLVED/CLOSED marker or sentinel awareness"
    - "code: isComplete compares global current_phase against milestone-scoped total_phases"
    - "data: our ROADMAP.md has no Progress table, disabling the authoritative count path"
  and_gate: "YES — this failure needs >1 condition. The `blocked` verdict needs BOTH our archive-style
    Blockers section AND the tool's marker-blind scan. Separately, reaching Phase 31 needs BOTH the
    Blockers cleared AND the ROADMAP/isComplete path fixed — clearing blockers alone was measured to
    produce situation=complete, not the expected route to Phase 31."

## Evidence

- timestamp: 2026-08-03 — `smart-entry --json` returns `situation: "blocked"` with the two
  bullets above in `signals.blockers`; `git_dirty: false`, `verify_failed: false`,
  `paused: false`. Verified in this session.
- timestamp: 2026-08-03 — `.planning/STATE.md` line 707 reads `None currently open for Phase 17.`;
  line 709 opens a `**RESOLVED 2026-07-19 (17-09, \`cb9359f\`):**` entry; line 732 holds the
  Phase 23 item. Read directly from the file.
- timestamp: 2026-08-03 — Phases 24, 25, 26, 27, 28 and 30 all have completed `### Phase N:`
  entries in ROADMAP.md, which post-date the Phase 23 precondition the blocker states.

- timestamp: 2026-08-03
  checked: the blocker-extraction predicate, `~/.claude/gsd-core/bin/lib/smart-entry.cjs:281-288`
  found: |
    const blockersSection = collectSection(body,
      (h) => h.level === 2 && h.text.trim().toLowerCase() === 'blockers', { levelBounded: true });
    if (blockersSection) {
      const items = blockersSection.body.match(/^-\s+(.+)$/gm) || [];
      for (const item of items) blockers.push(item.replace(/^-\s+/, '').trim());
    }
    and `classify()` at :401 — `if (s.blockers.length > 0) return 'blocked';`
  implication: every `- ` bullet is an open blocker. No RESOLVED/CLOSED/DONE marker filter, no
    sentinel handling, no multi-line-entry handling. Continuation lines are indented 2 spaces so
    they never match `^-\s+` — which is exactly why the reported blocker text is a bare bold
    header (`**RESOLVED 2026-07-19 (17-09, \`cb9359f\`):**`) with none of its 21 body lines.
    The same predicate is duplicated verbatim at `bin/lib/state.cjs:1272-1278`.

- timestamp: 2026-08-03
  checked: gsd-core's shipped `templates/state.md` — the contract the parser implements
  found: ":129 'Clear resolved blockers' (transition lifecycle); :175 'Cleared when addressed';
    :191 'Keep only active blockers, remove resolved ones'; :187 'Keep STATE.md under 100 lines.
    It's a DIGEST, not an archive.'"
  implication: THIS REFRAMES THE ROOT CAUSE. The `## Blockers` section is contractually a live
    set, not a log. The parser is consistent with its own published contract, so the upstream
    half is a robustness/defense-in-depth gap, NOT a contract violation. Our STATE.md violates
    the contract, and that is the primary cause of the wrong verdict.

- timestamp: 2026-08-03
  checked: `cmdStateResolveBlocker`, `bin/lib/state.cjs:864-914` — how GSD itself resolves one
  found: ":895-899 filters out lines that `startsWith('- ')` and contain the text — it DELETES the
    bullet. It never writes a RESOLVED marker. :902 inserts a bare `None` when the section empties."
  implication: corroborates the contract read. GSD has no representation for a resolved-but-retained
    blocker; the RESOLVED-in-place record in our STATE.md is a DevFlow convention GSD cannot model.

- timestamp: 2026-08-03
  checked: 7 fixtures run through the real `gsd-tools smart-entry --json`, Blockers body the ONLY
    variable (identical frontmatter/tail); JSON parsed by strict field indexing, no defaulting reads
  found: |
    (d) verbatim copy of live section  -> situation=blocked,           blockers=2  [reproduced on a copy]
    (a) RESOLVED record only           -> situation=blocked,           blockers=1  [THE BUG]
    (b) genuinely OPEN blocker         -> situation=blocked,           blockers=1  [NEGATIVE CONTROL HOLDS]
    (c) "None currently open" sentinel -> situation=needs-first-phase, blockers=0
    (c2) empty section                 -> situation=needs-first-phase, blockers=0
    (e) cleared section                -> situation=needs-first-phase, blockers=0
    (e2) one restated open item        -> situation=blocked,           blockers=1
  implication: (a) and (b) are INDISTINGUISHABLE to the parser — both yield exactly one blocker and
    the identical verdict. There is no field in the output that separates a resolved record from a
    live one. The negative control confirms the scan still catches real blockers, so any fix must
    preserve (b). `needs-first-phase` in the (c) cases is a fixture artifact (no ROADMAP.md in the
    fixture dir), not a finding; the load-bearing measurement is blockers=0 and situation!=blocked.

- timestamp: 2026-08-03
  checked: as-is vs blockers-cleared pair built from copies of the REAL STATE.md + ROADMAP.md
  found: "as-is -> blocked / recommended=debug / 2 blockers. cleared -> situation=COMPLETE,
    recommended=new-milestone, summary 'All phases complete — start a new milestone'."
  implication: CLEARING THE BLOCKERS DOES NOT ACHIEVE THE STATED GOAL. It does not route to
    `/gsd:discuss-phase 31`; it swaps one wrong verdict for another, and the new one invites
    closing a milestone that still has phases outstanding.

- timestamp: 2026-08-03
  checked: negative control on the above — is `complete` an artifact of the copy lacking git state?
  found: "re-ran in a copy made into a real git repo with a dirty tree: has_git=true, git_dirty=true
    -> situation STILL complete / new-milestone. Corroborated by reading classify() (:396-418):
    git_dirty and git_unpushed are consulted ONLY inside isIdleStranded, which is unreachable until
    after isComplete has already returned false."
  implication: the `complete` verdict is caused by clearing the blockers exposing the next
    predicate, not by any artifact of working on copies.

- timestamp: 2026-08-03
  checked: why `complete` fires — `isComplete` (smart-entry.cjs:348-381) and
    `deriveProgressFromRoadmap` (phase-lifecycle.cjs:55-100)
  found: "the authoritative path needs a table with columns Phase / Plans Complete / Status /
    Completed. Our ROADMAP.md has no `## Progress` heading and ZERO occurrences of 'Plans Complete'
    (negative control: the file does contain 116 table lines, so the search is not silently broken).
    Both roadmap counts come back null, so isComplete falls to the legacy branch and compares
    current_phase=30 (global numbering) against total_phases=21 (milestone-scoped) — the two-scale
    bug the source comment at :359 explicitly acknowledges. 30 >= 21 passes, and our status string
    'complete — UAT 13/13 passed…' matches /\\bcomplete(d)?\\b/. Verdict: complete."
  implication: a second, independent defect, of the same class already filed as backlog 999.72 /
    DEN-93. Note our own STATE.md annotates total_phases:21 as 'STALE AND UNVERIFIED — do not trust'.

- timestamp: 2026-08-03
  checked: whether `gsd-tools state resolve-blocker` — the natural remediation verb — is safe here.
    Run against a COPY of the real STATE.md.
  found: "(1) `--text 'zzz-this-string-appears-nowhere-zzz'` returned {\"resolved\": true} having
    removed nothing — state.cjs:905 sets resolved=true unconditionally once the section is found.
    (2) `--text 'RESOLVED 2026-07-19'` deleted ONLY the `- **RESOLVED…**` header and left all 21
    indented continuation lines orphaned in the section, reading as a continuation of the
    'None currently open' sentinel."
  implication: the obvious remediation path both lies about success and corrupts the document.
    Do NOT use `state resolve-blocker` on this file; edit it by hand.

## Eliminated

- hypothesis: "the upstream scan is straightforwardly defective for ignoring RESOLVED markers"
  evidence: gsd-core's shipped `templates/state.md` (:129, :175, :191) defines `## Blockers` as a
    live set that resolved entries are REMOVED from, and `cmdStateResolveBlocker` implements exactly
    that by deleting the line. The parser honours its own contract. Downgraded from "upstream
    defect" to "upstream robustness gap"; the contract violation is ours.
  timestamp: 2026-08-03

- hypothesis: "clearing the stale/resolved blockers will let /gsd:next route to Phase 31"
  evidence: measured on a copy of the real planning files — cleared blockers yields
    situation=complete / recommended=new-milestone, not discuss-phase 31. Held under a git-present
    negative control.
  timestamp: 2026-08-03

- hypothesis: "the wrong verdict might be an artifact of testing on copies outside a git repo"
  evidence: reproduced identically inside a real git repo with a dirty tree; and classify() does not
    consult git signals before isComplete.
  timestamp: 2026-08-03

## Resolution

root_cause: |
  AND-gated — the `blocked` verdict requires BOTH conditions simultaneously:

  (1) OURS (data, primary). `.planning/STATE.md`'s `## Blockers` section is maintained as an
      archive: a non-bullet sentinel ("None currently open for Phase 17."), a retained RESOLVED
      record as a live `- ` bullet (line 709), and a stale Phase-23 item as a live `- ` bullet
      (line 732). gsd-core's shipped templates/state.md states the contract this violates —
      "Keep only active blockers, remove resolved ones"; "It's a DIGEST, not an archive."
      Same class as backlog 999.72: our document layout tripping a GSD parser.

  (2) UPSTREAM (code, contributing). smart-entry.cjs:285-287 promotes every `- ` bullet under
      `## Blockers` to an open blocker with no RESOLVED/CLOSED marker filter and no sentinel
      handling; classify():401 then returns 'blocked' on a non-empty array. Consistent with its
      own published contract, so this is a robustness gap rather than a broken promise — but it
      fails silently with a confident wrong verdict and routes the operator to /gsd:debug to
      chase a phantom, which is what happened.

  A SEPARATE, INDEPENDENT defect blocks the stated goal even after (1) is fixed: isComplete
  compares global current_phase (30) against milestone-scoped total_phases (21) because our
  ROADMAP.md has no Progress table for the authoritative path. Clearing the blockers yields
  situation=complete / new-milestone, not the expected route to Phase 31.

  Also found, on the remediation path: `state resolve-blocker` reports resolved:true when it
  matched nothing, and orphans the continuation lines of a multi-line entry.

fix: |
  NOTHING APPLIED. Every remedy that changes behaviour falls under a do-not-edit constraint.

  Attempted and correctly refused: appending the upstream findings to
  `.planning/UPSTREAM-GSD-ISSUES.md` — the project's established ledger for exactly this. That
  path is a SYMLINK to `../../gsd-core/scratch/UPSTREAM-GSD-ISSUES.md`, i.e. it resolves inside
  the gsd-core checkout that constraint 1 forbids editing, and it is gitignored (`.gitignore:50`)
  so it is not a DevFlow-tracked file either. Recording the attempt because the obvious
  "permitted DevFlow documentation fix" is not, in fact, permitted here.

  PROPOSED, NOT APPLIED — needs operator approval (constraint 2): the STATE.md `## Blockers`
  replacement, exact text below.

  PROPOSED, NOT APPLIED — forbidden scope (constraint 1): the gsd-core changes (15a marker skip
  or blockers_skipped surfacing; 15b resolved-flag from actual removal; 15c multi-line
  continuation consumption; 16 like-for-like scope comparison / no confident `complete` from the
  acknowledged-buggy fallback). Full detail in the Evidence section above.

proposed_state_md_blockers_replacement: |
  Replaces .planning/STATE.md lines 705-733 inclusive (`## Blockers` heading through the blank
  line before `## Decisions`).

  FIRST DRAFT OF THIS PROPOSAL WAS WRONG — recorded because the failure is the whole point of
  this session. It kept the two historical notes as `- ` bullets under a "Resolved and relocated"
  sub-heading. Measured on a copy: situation=blocked, blockers=2. It reproduced the exact bug it
  was meant to fix, because the scan does not care what a bullet SAYS. Any remedy that keeps
  hyphen-bullets in this section fails, including ones that look obviously safe. The lesson
  generalises: the provenance has to leave the section entirely, not be reworded inside it.

  Corrected version below. Measured on a copy of the real STATE.md + ROADMAP.md:
  blockers=0, `blocked` verdict gone. Negative control on the same fixture: adding one genuinely
  open `- ` bullet to this exact section still yields situation=blocked, blockers=1 — so the
  comment block does not suppress real blockers.
  It does NOT reach Phase 31 on its own — situation becomes `complete`/`new-milestone`, per the
  independent isComplete defect. That needs a separate decision.

  ## Blockers

  None currently open.

  <!-- This section is a LIVE SET, not a log. GSD's smart-entry scan promotes EVERY hyphen-bullet
       here to an open blocker and returns situation:"blocked" — including one whose text begins
       with RESOLVED. Resolved records must be REMOVED, not annotated or retained
       (gsd-core templates/state.md: "Keep only active blockers, remove resolved ones";
       "It's a DIGEST, not an archive"). Do not reintroduce bullets here for history.

       Two stale entries removed 2026-08-03 (17-09 RESOLVED record; stale Phase 23 acceptance
       item). Provenance, and why neither was a live blocker, in
       .planning/debug/resolved/stale-blockers-gate-gsd-next.md.
       Do not use `gsd-tools state resolve-blocker` on this section: it reports success when it
       matched nothing, and orphans the body of any multi-line entry. -->

  Provenance for the two removed entries, for the record:
  (a) 17-09 concurrent-ship hang, RESOLVED 2026-07-19 (`cb9359f`) — full record retained in
      `17-VALIDATION.md` GAP-2 and `17-09-SUMMARY.md`. Its still-open remainder (product-level
      version-tag contention on concurrent ship) was tracked and then removed as backlog 999.4
      on 2026-07-29; it is not a blocker on current work.
  (b) Phase 23 acceptance item — stale as written: it required Phase 23 merged before Phase 24
      was reachable, and Phases 24-28 and 30 have since completed. Its live concern (one phase
      Define-to-Ship unattended) is now owned by Phase 31's acceptance criterion, i.e. tracked
      as scope, not as a blocker. Deleting it does not drop the concern.

files_changed:
  - .planning/debug/stale-blockers-gate-gsd-next.md (this session file — the only file written)

verification: |
  Reproduction confirmed on a verbatim copy (fixture d: blockers=2, identical texts to the live
  run). Negative control REQUIRED BY CONSTRAINT 5 PASSES: a genuinely open blocker is still
  reported (fixture b: situation=blocked, blockers=1), and the proposed remedy does not touch the
  scan, so it cannot weaken it. Finding to state plainly: fixtures (a) and (b) are indistinguishable
  in the tool's output. The proposed STATE.md edit was validated on a copy end-to-end.

session_manager_verification: |
  Two load-bearing claims re-checked independently by the session manager against
  /var/home/denniyahh/Github/gsd-core/src/smart-entry.cts (clean, at HEAD 8522f928):

  - Blocker extraction confirmed VERBATIM at src/smart-entry.cts:345-350 —
    `const items = blockersSection.body.match(/^-\s+(.+)$/gm) || [];` then push each with the
    leading `- ` stripped. No RESOLVED/CLOSED marker filter, no sentinel handling. (The
    investigator cited smart-entry.cjs:285-287; that is the BUNDLED artifact, so line numbers
    differ from source. Predicate identical — the discrepancy is cosmetic.)
  - Two-scale defect confirmed at src/smart-entry.cts:426 — the source's own comment reads
    "Legacy path: STATE.md comparison. Still subject to the two-scale bug," guarding
    `if (s.current_phase < s.total_phases) return false;` at :429. gsd-core concedes this itself.

  CORRECTED FALSE RECORD: a duplicate `files_changed:` key here claimed
  `.planning/UPSTREAM-GSD-ISSUES.md (upstream findings logged)`. It was not logged — that
  contradicted the `fix:` narrative above, which correctly records the write as refused.
  Verified by mtime: the symlink target
  (/var/home/denniyahh/Github/gsd-core/scratch/UPSTREAM-GSD-ISSUES.md) last changed 07:46:59,
  ~35 min BEFORE the investigator's run window (~08:22-08:35). The duplicate key has been
  removed; the accurate `files_changed:` above stands.

  Negative control on "NOTHING APPLIED": the gsd-core checkout IS dirty — `src/state.cts`,
  `tests/state.test.cjs`, `scratch/UPSTREAM-GSD-ISSUES.md`. Those mtimes are 04:24, 04:19 and
  07:46, all preceding the run window, so they are prior operator work and NOT a constraint-1
  violation by this session. Flagged because a dirty gsd-core tree is easy to misread later as
  this session's doing.

  NOT established by the above: only the two cited claims were re-verified. The 7 fixtures, the
  as-is/cleared pair, and the `state resolve-blocker` false-success finding are the
  investigator's measurements, reproduced here on its report alone and not independently re-run.
