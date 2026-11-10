## CONFIRMED

### C1 — Critical — a finished run can contain source files no report ever covered, with no gate naming them at the finish
**`.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:186`** — "Clear the base only when a remainder computation comes back empty, on a finish (the D-41 approve included), **on the transition to the gap loop**, and on abort"
and **`52-33-PLAN.md:164`** (test `a_reverify_that_leaves_the_fix_uncovered_gates_instead_of_validating`, digest-match control) — "a `stale_after_reverify` gate recording `gaps_found` and naming `src/calc_cli.py`, on which a hand-written approve **sets the gap marker, launches the gaps Plan and clears `code_fix_base`**"

Sequence:
1. Validate passes clean; operator rejects with a note → quick fix adds `src/calc_cli.py` (not in `covered_files`); `code_fix_base = B` (52-29:54).
2. Quick pass ends: status `gaps_found`, remainder `{src/calc_cli.py}` non-empty → re-verify pass (52-33:24).
3. Post-check: `gaps_found`, D-11 digest **matches** (52-33:164) → `stale_after_reverify` gate over `gaps_found`, context names `src/calc_cli.py`, 1 counted failure.
4. Operator approves. Under D-37 "`gaps_found` → the gap loop" (52-29:33) this launches Plan `--gaps`, **and the plan's own test says the base is cleared**.
5. Gap loop: Plan `--gaps` → Code `--gaps-only` → Validate. GSD's verifier lists what it chooses → `passed`.
6. `clean_pass` gate → operator approves → pickup re-read checks **status only**: 52-33:248 — "The re-read does not recompute an uncovered remainder". Run records `{"reason":"finished"}`.

Result: the run finishes with `src/calc_cli.py` never in `covered_files`, and the only gate that ever mentioned it said "approve = run the gap loop" (D-36/D-09 meaning), not "accept these files". D-41's knowingly-accept mechanism is scoped to the `passed` gate whose approve *finishes*; the `gaps_found` branch of the same writer has no such acceptance step, and clearing the base at step 4 discards the only record of what was uncovered. 52-33's own success criterion ("No quick fix reaches Validate's pass unless the report on disk covers what it changed") is false after step 4.

Evidence checked: 52-33:29, 31, 164, 186-187; 52-29:33; CONTEXT D-36, D-41; 52-27:30 (gap marker set/cleared rules).
Fix: either keep `code_fix_base` (and re-check the remainder) until a report covers it — the gap loop's own verifier pass is the natural point — or make the gap-loop approve an explicit acceptance that names the uncovered files, and carry those files into the terminal finish gate's context and the `workflow_finished` payload.

### C2 — High — "unusable base → replace with HEAD" silently drops every earlier quick fix's uncovered files
**`52-33-PLAN.md:183-184`** — "an absent base, one naming no commit, or one that is not an ancestor **is replaced by HEAD**. Any other exit status or a spawn failure fails closed … whose context names the check that failed."
**`52-33-PLAN.md:29`** — "the remainder is the union of every quick fix that no report has covered and no approve has accepted … It records a fresh base at HEAD only when the base itself is unusable"

Sequence:
1. Quick fix 1 adds `src/calc_cli.py` outside `covered_files`, base `B`.
2. The re-verify (or a later fix) agent amends/rebases; `B` is no longer an ancestor.
3. Post-check remainder errors → `reader_failure` gate (not counted, 52-33:31).
4. Operator **rejects** that gate ("try again"). Reject path: `git merge-base --is-ancestor B HEAD` exits 1 → "not an ancestor" → base replaced by HEAD, launch, **no gate, no event, no message**. (52-33:184's "the base is never replaced silently" covers only the *cannot-run* branch.)
5. Quick fix 2 touches only covered files → remainder empty → `code_fix_base` cleared → Validate → `passed` → finish.

`src/calc_cli.py` is still in the tree, uncovered, and no gate ever named it. The protection D-33 exists to give is dropped by an agent action (rewriting its own history) that the operator is never told about.

Secondary defect in the same block: the two probes classify the *same* git state differently — 52-33:366 records the operator's own probe table as "absent sha 1/128, a blob 1/128" for `rev-parse`/`is-ancestor`. An absent/replaced base therefore reads as "unusable → replace" through `rev-parse` and as "cannot run → fail closed" through `is-ancestor`; which behaviour ships depends on call order, and nothing tests the ordering.

Evidence checked: 52-33:29, 31, 182-188, 366; CONTEXT D-33; 52-29:54 (`code_fix_base` recorded at HEAD).
Fix: treat "not an ancestor / not a commit" as a *gate* (same fail-closed branch), not a silent HEAD reset — or persist the leftover uncovered paths in state so a base replacement cannot lose them.

### C3 — High — `disagreement_over_passed` approves into a finish, contradicting D-08 and 52-22
**`52-29-PLAN.md:33`** — "`passed` → finish … **for every verification kind**, including a failure-cap gate over a real pass"
**`52-22-PLAN.md:30`** — "`passed` or `human_needed` → a gate whose context says the agent and GSD disagree; **never a finish**"

`disagreement_over_passed` is the kind 52-22 opens for "agent fail + `passed`" (52-22:34). Under 52-29's router that gate records `gsd_status = passed`, so approve → `workflow_finished`. D-08 is explicit and was **not** amended by D-37: "The Validate verdict is **pass only if** the validate-phase agent says pass **and** GSD's own reader … returns `passed`". An agent that said FAIL is not a pass. D-37's context sanctions exactly one extension of the finish rule ("including a failure-cap gate over a real pass") and does not mention the disagreement kind.

Sequence: Validate agent reports failure; GSD reports `passed`; gate opens; operator approves; run finishes `finished` though no stage ever delivered a pass verdict — and 52-22's own truth, still in the plan set, says that must never happen. The same applies to `ambiguous_verdict` recording `passed`.

Evidence checked: 52-22:30, 34, 121 (test `an_agent_fail_over_a_passed_report_gates_as_a_disagreement`, which asserts only that a gate opens); 52-29:33, 188; CONTEXT D-08, D-37. No test pins the contradiction on either side.
Fix: needs an operator decision. Either restore 52-22's "never a finish" as an explicit arm (`DisagreementOverPassed → refused`), or record it as an amendment to D-08.

### C4 — High — a `reader_failure` gate has no answer that can make progress, and no counter bounds the loop
**`52-29-PLAN.md:188`** — "`Status(HumanNeeded)`, stale `gaps_found`, **reader failure → refused** (reject or abort)"
**`52-29-PLAN.md:191`** — "reject: **every verification basis** → `/gsd-quick` with the note"
**`52-33-PLAN.md:31`** — "this plan's `reader_failure` gate **counts none**, including one from inside the digest check"

Sequence:
1. `gsd-tools` is missing / exits non-zero / prints garbage (D-24's cases), or `covered_files` cannot be read.
2. `reader_failure` gate opens, no `gsd_status`.
3. Approve: refused. Reject: `/gsd-quick` with a note — a *source-code* fix for an infrastructure failure; it cannot restore the reader.
4. Validate (or Code) re-runs, the read fails again, the gate reopens. `phase_validate_failures` never grows, so `MAX_PHASE_VALIDATE_FAILURES` (10) never fires; `consecutive_failures` never grows either.
5. Only `abort` escapes.

D-24's stated property is "never a pass and never a silent retry". This is a *loud* retry, but an ineffectual, unbounded one, and the operator's natural answer ("fix the install, then retry the read") is not offered anywhere: for a `stage_failure` gate approve = retry, for a reader failure there is no retry answer at all. This is a gate whose offered answers contradict the cause it reports.

Evidence checked: 52-29:188, 191, 35; 52-33:31, 30; 52-22:30 (D-24 gate shape); CONTEXT D-24; `mode.rs` L18/L41 (caps).
Fix: give the `reader_failure` basis an approve/non-destructive "retry the read" answer (52-39 already makes `stage_failure` approve a retry), and count those passes against the ceiling like any other Validate failure.

### C5 — Medium — three gate families count zero Validate failures, so loops through them can never reach either ceiling
`52-28-PLAN.md:25` — "`human_needed` → 52-22's `human_needed` gate kind … **counted as 52-22 counts it, which is none** (52-22's `human_needed` gate follows an agent pass … at b8df457 the streak update in `handle_validate_outcome` runs only when Validate's result is `Failed`)"; 52-33:31 for `reader_failure`; 52-28 Task 2 for the Validate-path `stale_gaps_found` (no count stated anywhere in 52-28).

For all three the only non-abort exit is reject → `/gsd-quick` → Validate → same gate. D-12's backstop (`MAX_PHASE_VALIDATE_FAILURES` = 10) is described as the guard against exactly this ("the 10-per-phase ceiling stays as the backstop", D-25) and is inert here. The loop is operator-paced, so it is not an unattended DoS — but the answer to "can a loop never reach the failure ceiling" is yes, for the three high-frequency verdicts, and no plan says so.

Evidence checked: 52-28:25, 52-28 Task 2 behaviour (no count), 52-33:31, `crates/devflow-cli/src/pipeline_outcomes.rs` L578-700 (counter block runs only when `result == ValidateResult::Failed`), `mode.rs` L206-207.
Fix: state one rule ("a gate that only offers reject/abort counts one Validate failure per round") and apply it to these three.

### C6 — Medium — `code_fix_base`'s clear points disagree between 52-29 and 52-33, and the disagreement is at the exact transition 52-33 depends on
**`52-29-PLAN.md:54`** — "record `code_fix = Quick`, the note, and `code_fix_base` … a new … State field, **cleared with `code_fix`**"
**`52-33-PLAN.md:186-187`** — "Clear the base only when a remainder computation comes back empty, on a finish …, on the transition to the gap loop, and on abort; **nothing else clears or replaces it**."

`code_fix` is cleared on *every* transition, both at the `--until` halt and in the normal path — `crates/devflow-cli/src/pipeline_gate.rs:86` and `:113-116` (`s.stage = to; … s.code_fix = None;`). So 52-29's literal rule drops the base on the Code→Validate transition, i.e. before the post-check 52-33 adds. 52-29 hedges for the *later reject* case only ("from 52-33 on, a later quick reject keeps a base already set"), and 52-33 does not name a 52-29 test to change for the transition. Two plans own the same field's lifetime with directly opposed text.

Evidence checked: 52-29:54; 52-33:29, 186-187; `pipeline_gate.rs` L86, L113-116.
Fix: 52-29 should state the lifetime as 52-33's, or 52-29 should not touch the field beyond `Some(HEAD)` at reject; add the 52-29 test row that pins survival across Code→Validate.

### C7 — Medium — the non-UTF-8 row cannot fail on the defect it claims: an index entry is invisible to `git diff <base>..HEAD`
**`52-33-PLAN.md:162`** — "a fixture path whose name holds a byte that is not UTF-8 … **added through the index with `git update-index --add --cacheinfo`**, so no filesystem has to accept the name), so `git diff -z` prints a non-UTF-8 path"

Verified, with a control (same repo, same blob, only the commit differs):
- `git update-index --add --cacheinfo 100644,<blob>,"src/calc_\xff.py"` then `git diff --name-only -z <base>..HEAD` → **empty output**.
- After `git commit`: `git diff --name-only --no-renames -z <base>..HEAD` → `src/calc_\377.py`.

`git diff A..B` is a commit-to-commit diff; index entries are not in `HEAD`. So row (f) as written yields an empty remainder → the plan's own correct behaviour (follow GSD's status → no `reader_failure` gate) → the row's assertion fails on correct code, and the non-UTF-8 failure path (which *is* reachable: a committed path with invalid UTF-8) stays untested.

Evidence checked: live probe on git 2.56.0 (scratch repo, since removed); `52-33:162`.
Fix: add the commit step (`git update-index` then `git commit -m fixture`) and note that only the tree entry needs the byte, not the worktree.

### C8 — Medium — a 52-29 gate asserts a note reaches the Plan/Validate prompt through a field that arrives three waves later
**`52-29-PLAN.md:166`** — "for `stage_failure` at Plan, Code and Validate it relaunches that stage **with the note** (at Code in `code_fix_note`)", with fails_when "a stage-failure reject did not relaunch its stage with the note"
**`52-29-PLAN.md:34`** — "at Plan and Validate through **52-31's** `State.relaunch_note`"
**`52-29-PLAN.md:315`** (same plan, verification limits) — "between this plan and 52-31 a stage-failure reject at Plan or Validate **has no prompt carrier for its note** beyond what 52-26 left"

Plan order is 28 → 29 → 30 → 33 → 31 → 32. At 52-29's gate there is no carrier, so the assertion in 52-29:166 cannot pass without 52-29 inventing the field 52-31 claims to add — and if it does, 52-31's Task 4 either duplicates it or silently keeps a second field. Either way GATE-01/D-14's "verbatim on every launch shape" is violated for two waves by 52-29's own admission, while the same plan's test claims it holds.

Evidence checked: 52-29:34, 166, 315; 52-31:36, 232 (Task 4: "commits its note to `State.relaunch_note` before `relaunch_failed_stage`"), 52-31 Task 4 test `a_rejected_stage_failure_at_plan_or_validate_relaunches_with_the_note`.
Fix: move the Plan/Validate note carrier (or the whole stage-failure reject route) into 52-31, and drop the Plan/Validate rows from 52-29's test.

### C9 — Medium — the answer screen's position relative to `claim`, `gate_pending` and `gate_resolved` is unspecified, so a refused answer can still publish a resolution
**`52-29-PLAN.md:39`** — "one screen in the supervisor's answer pickup checks every answer against the routers before any gate's handler sees it … it closes that answer, opens a never-silent gate … and emits `gate_answer_refused`"
**`52-29-PLAN.md:205`** — "The `gate_resolved` event carries the gate's `gate_kind` and the name of the route the answer runs, from the same function."

The pickup in `run_gate_with_timeout` already does, in order: read the answer → `Gates::claim` (`pipeline_gate.rs:640-655`) → `commit(state, |s| s.gate_pending = false)` (`:680`) → `gate_resolved` (approved, action, responded_by) (`:681-695`) → return the action. "In the answer pickup" does not say whether the screen runs before the claim or after the event. Placed after, a refused approve still (a) consumes the claim so the operator's answer cannot be re-written under the same gate, (b) clears `gate_pending` while the gate is being reopened, and (c) emits `gate_resolved` naming a route that never ran — the exact 999.189 failure ("what is printed is what runs") inverted. No test asserts the absence: 52-29's bypass rows and 52-33's `an_approve_to_finish_rereads_gsd_status_at_pickup` assert `gate_answer_refused`, "nothing launches", and the reopened gate, but never "no `gate_resolved` for the refused answer" and never the claim state.

Evidence checked: `pipeline_gate.rs` L639-700; 52-29:39, 41, 166, 241; 52-33:164 (`an_approve_to_finish_rereads_gsd_status_at_pickup`).
Fix: pin the order in the plan (screen before `Gates::claim`) and add "no `gate_resolved`, no `gate_pending` write, answer left unclaimed" to the bypass rows.

## SUSPECTED

### S1 — Medium (speculation) — an approve into the gap loop never runs the D-11 digest check, so a report that went stale during the wait is looped on
D-11's check lives in the Validate arm (52-28 Task 2: "Before taking the gap loop on `gaps_found`") and in 52-33's pickup re-read — but the re-read runs "only for an approve whose route is finish" (**52-33:248**: "No other route re-reads GSD's status"). So an operator's approve of a `gaps_found` gate runs the gap loop over the status recorded when the gate opened, with no digest check even though the gate may have waited up to `DEVFLOW_GATE_TIMEOUT_SECS` (3 days). Window requires an outside commit landing on the phase branch during the wait (nothing in DevFlow commits while a gate is open), which is why this is a suspicion rather than a confirmed break. Fix: extend the re-read to the gap-loop route, or state explicitly that only the finish route is re-read and why the gap route is exempt.

### S2 — Low (speculation) — the routing fields are read from a file the run's own agent can write
52-29's routers read `gate_kind`/`gsd_status` from the gate file "read back from disk" (52-29:39, 205), and gates live under `.devflow/gates/` in the project root. T-52-51 considers only that `context` "can hold agent text"; it does not consider that a Code/Validate agent with shell access to the project root could set `gate_kind: clean_pass`/`gsd_status: passed` on an open gate and turn a refused approve into a finish (the operator still wrote `approve`, so the exploit is narrow, but it converts "approve over `human_needed` is impossible" into "possible"). Not verified against the sandbox's actual write permissions. Fix: state the assumption (gate files are outside the agent's writable surface) and test it, or cross-check the recorded kind/status against a DevFlow-owned copy in state.

### S3 — Low (speculation) — `--until validate` becomes a silent no-op
**`52-27-PLAN.md:113`** — "Make `transition`'s `--until` halt skip `from == Stage::Validate`"; acceptance criterion at :126. D-07 keeps `validate` as an accepted `--until` value. Since a pass ends in `finish_run` (52-08) and a fail now loops, `--until validate` stops nothing; before this change a `gaps_found` verdict at least produced `workflow_finished {"reason":"stopped_at","stage":"validate"}`. An operator asking to stop after Validate now gets an unattended loop and a `finished` run with no `stopped_at`. Fix: print/record that `--until validate` is equivalent to no halt (fail-closed observability), or reject the value.

## SIMPLIFY

1. **Replace `code_fix_base` + ancestry with a persisted uncovered-path set.** Every property 52-33 spends two truths, a `Result` function, two git probes, an exit-code taxonomy, a four-point clear list and four tests pinning is "which paths no report has covered". `State.uncovered_paths: BTreeSet<String>` (union on each quick pass's end, subtract the report's `covered_files`, clear on empty/finish/abort) gives the same property, kills C1's base-clearing hole and C2's silent-drop entirely, and removes the ancestry question from the failure surface.
2. **Collapse the two git probes.** 52-33's own probe table (52-33:366) shows `rev-parse --verify --quiet <base>^{commit}` and `merge-base --is-ancestor` both fail (1 and 128) on absent/blob bases. One call with "0 = usable, 1/128 = unusable → gate, else fail closed" is equivalent and removes the order-dependent classification in C2.
3. **Derive "auto relaunch used" instead of storing a flag plus a gate id plus `responded_by` discrimination.** 52-32 spends `State.rate_limit_auto_relaunch_used`, `State.rate_limit_timer_gate`, two `responded_by` comparisons and two dedicated crash tests on "has a timer already answered a rate-limit gate in this run". The answer file (`responded_by` naming the timer) plus the `gate_resolved` event already record it; one durable lookup replaces the two-field protocol and its four enumerated crash states. (Caveat: answers are reaped — `gates.rs:680 reap_answer` — so the lookup must read the event log, not the answer slot.)
4. **Cut redundant status reads.** The Validate verdict, 52-28's post-check, 52-33's quick-pass end, 52-33's post-check and 52-33's pickup re-read each issue their own `query verification status`, which is why 52-33 needs a "One commit" truth, a named-replies table and per-test stub surgery ("consumes one reply from an earlier test's scripted `stub_gsd_run`"). Record the last read in state (with the tree hash it was read at) and let the post-check/pickup compare, or funnel the reads through one helper that the stub keys by call site rather than by call order.

## COVERAGE
Read in full: `52-33-PLAN.md`, `52-28-PLAN.md`, `52-29-PLAN.md`, `52-32-PLAN.md`, `52-27-PLAN.md`, `52-22-PLAN.md`, `52-CONTEXT.md`.
Skimmed (grep-targeted only): `52-30-PLAN.md`, `52-31-PLAN.md`, `52-08-PLAN.md`, `52-VALIDATION.md`, `52-REVIEWS.md`.
Source read: `crates/devflow-cli/src/pipeline_gate.rs` L40-140, L400-760, L870-960; `crates/devflow-cli/src/pipeline_outcomes.rs` L470-700; `crates/devflow-core/src/mode.rs` (counter/cap symbols). Not read: `gates.rs`, `state.rs`, `gsd_reader.rs`, `reset_time.rs` internals — so C6's field-lifetime conflict is argued from the plans and `pipeline_gate.rs` only, and S2's writability assumption is unverified.
Read-only respected: no tracked file was modified; the one scratch repo used for the git probe (C7) was created outside the snapshot and deleted (`gitprobe` absent, verified).
