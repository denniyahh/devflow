## CONFIRMED

Convention: `$P` = `.planning/phases/49-live-unattended-run-the-milestone-s-instrument` (relative to the snapshot root). All line numbers are in the snapshot.

### 1. The SURV-01 gate fails the honest statement and passes the overstatement (negative control run)

`$P/49-12-PLAN.md:173`:
```
bad=$(rg --count-matches -i -e "confirmed" -e "no interleaving" "$L/surv" || true); echo "surv_verdict_words=${bad:-0}"; [ "${bad:-0}" -eq 0 ] || rc=1
```
The write-up must state the substance of `REQUIREMENTS.md:221-222` ("No interleaving has been observed in the wild; … A run with no observed interleaving confirms nothing and must not be written up as if it did"), and `$P/49-12-PLAN.md:166` orders it: "state plainly that one run with no observed overlap establishes nothing about the Phase 48 guarantee". I ran the gate's own expression (`rg --count-matches -i -e "confirmed" -e "no interleaving"`) over two candidate SURV-01 sections:

```
writers=2 … interleaved_windows=0
As the REQUIREMENTS.md SURV-01 note puts it: no interleaving has been observed in the wild,
so this run's single observation confirms nothing …      -> verify_word_count=1   (gate FAILS correct work)

writers=2 … interleaved_windows=0
The Phase 48 guarantee is upheld by this run; the two writers never overlapped …  -> verify_word_count=0   (gate PASSES the overstatement)
```
Impact: correct, requirement-echoing prose is rejected; a claim that the Phase 48 guarantee is upheld passes, because the ban is a substring list, not a claim shape. Note the same expression at `$P/49-04-PLAN.md:139` is safe only because 49-04 mandates an exact closing sentence; the write-up has no such mandated sentence.
Recommendation: require `49-LIVE-RUN.md` to contain 49-04's verbatim closing sentence (`This is one observation of one run; it establishes nothing about SURV-01's guarantee either way.`) and replace the substring ban with an assertion-shape ban, e.g. `rg -i -e '(no interleav|never (interleav|overlapp))[^.]*\b(is|was|has been)\b[^.]*\b(confirmed|proven|ruled out)\b'`.

### 2. `--attempt-gate`'s DECN-03 cross-check cannot tell "no event" from "jq failed", and then *forces* the weakest classification

`$P/49-12-PLAN.md:173`:
```
n=$(jq -c "select(.event == \"checkpoint_auto_decided\" and .phase == 1)" "$E" | wc -l) … if [ "$n" -eq 0 ]; then [ "${pne:-0}" -eq 1 ] || rc=1; else [ "${pne:-0}" -eq 0 ] || rc=1; fi
```
`jq`'s exit status is discarded (`| wc -l`, no `pipefail`; `pipefail` appears nowhere in any gate — only in 49-03's style section) and `jq`'s presence is asserted nowhere in the phase (no `command -v jq` in any of the 12 plans). Measured:

```
good.jsonl (one matching event)  -> n=1
good.jsonl with jq unavailable   -> n=0   → gate then REQUIRES "**DECN-03 outcome: Path not exercised**"
truncated events.jsonl           -> n=1   (partial count from a file jq errored on)
```
Impact: a run that *did* exercise DECN-03 (the case the 47-observation standard exists for) is recorded as criterion-4 "path not exercised", and the gate's only cross-check ratifies it. `VERIFY-01` then closes criterion 4 on an artefact of the measuring environment.
Recommendation: `command -v jq` at the top; capture `jq`'s status separately; count a second, independent way (`rg --count-matches '"event":"checkpoint_auto_decided"'` on the raw file) and fail closed on disagreement — per RULE ZERO (b), a single count can never look wrong.

### 3. 49-12's own must_have ("the bare event is never enough for Followed") has no gate, so `Followed` can be written over a failed spawn

`$P/49-12-PLAN.md:24` requires the DECN-03 record to carry "the mechanical inputs it rests on (…) and which turn carried which rule; the bare event is never enough for Followed". `47-PHASE49-OBSERVATION.md:16-18` is explicit: "The event alone is insufficient: it is emitted before the spawn deliberately, so a spawn failure still records the decision attempt. Treating that event as proof of a decision would label a failed spawn as followed." The gate (`$P/49-12-PLAN.md:173`) enforces the **inverse** only: it forces `Path not exercised` when `n == 0` and merely forbids that one string when `n >= 1`. Everything else — the resumed-turn capture, the shared session id, the comparison reasoning, progression past the gate, the chosen option — is prose whose presence is checked only as three bare substrings (`CODE_STAGE_POLICY`, `checkpoint_auto_decide_prompt`, `session`).
Impact: with a single `checkpoint_auto_decided` line, `**DECN-03 outcome: Followed**` passes the gate with no capture at all — exactly the "Void" case the standard names.
Recommendation: make the class mechanical: `Void` when the resumed-turn capture is absent from `collected/`, `Followed` only with capture present + shared pseudonymized session id + an `advance`/`transition`-past-gate event, and fail on any other combination.

### 4. 49-11's only human gate is registered as `gate="blocking"` — the value the executor's dispatch prompt calls auto-approvable — while `workflow.auto_mode` is `true`

`$P/49-11-PLAN.md:76`: `<task type="checkpoint:human-action" gate="blocking">`, whose must_haves require a lane that "stops even with `workflow.auto_mode: true`". `.planning/config.json:8`: `"auto_mode": true`. The orchestrator's Pattern-A prompt for this repo (`_external/gsd-core/workflows/execute-plan.md:144`) says verbatim:
```
gate="blocking" (the default) is auto-approvable in auto-mode per the executor's own checkpoint protocol, gate="blocking-human" always surfaces to a human
```
while `_external/gsd-core/workflows/execute-phase.md:1047` says "**human-action** → Present to user", and `:1050` calls out `blocking-human` as the carve-out. The vocabulary is already in the phase: `$P/49-06-PLAN.md:70,183` uses `gate="blocking-human"` and its gate asserts the bytes. The plan cites only the orchestrator side ("verified there", `$P/49-11-PLAN.md` `<context>` → `49-RESEARCH.md:357`, which cites execute-phase.md:1047).
Impact: if the executor's protocol wins, the checkpoint can be auto-answered `done` in auto-mode, `<verify>` then runs with no operator present — and because the attempt evidence lives outside the repo and is never aged or deleted (`$P/49-08-PLAN.md:187-193` reads `current-attempt`; nothing invalidates a previous session's `shipped`), a passing verify from an earlier session closes the phase's one human gate with no operator involvement, then wave 6 commits a VERIFY-01 verdict.
**What this does not establish:** `$HOME/.claude/gsd-core/references/checkpoints.md` (the executor's own protocol, referenced at `execute-plan.md:350`) is not in the snapshot, so I could not read the deciding rule — the two shipped workflows contradict each other. The fix is one word: `gate="blocking-human"` (or execute this phase with `auto_mode: false`).

### 5. `cap_reached` is reachable from `verdict.json` files that no manifest covers and that are never bound to their directory

`$P/49-08-PLAN.md:192-193`: "For a stopped attempt count every `attempt-*/verdict.json` with `outcome` `stopped` and a non-empty `stop_note`: three or more → `cap_reached`." Per the published layout contract (`$P/49-03-PLAN.md:98`), `verdict.json` lives at `$EV/verdict.json` — *outside* `collected/`, so `MANIFEST.sha256` (checked with `(cd collected && sha256sum --quiet -c MANIFEST.sha256)`) does not cover it, and the spec never compares `verdict.json`'s own `attempt` field to the directory index. The only cap control is `$P/49-08-PLAN.md:172` ("a stopped attempt without a note does not count").
Impact: three hand-written JSON files — or one file copied into `attempt-1/`, `attempt-2/`, `attempt-3/` — reach the cap; the checkpoint closes with exit 0, 49-11 accepts `cap_reached`, and `$P/49-12-PLAN.md:132` requires `**VERIFY-01: not met — D-11 cap reached**`. No live attempt is needed and no gate notices. (This is also the only path that can un-wedge the phase if finding 6 is real, so it is not merely a hygiene issue.)
Recommendation: for every counted attempt require `attempt.json`, `prelaunch/check.txt` and `collected/{DONE,MANIFEST.sha256}` + `verdict.json` `.attempt == <dir index>`; add the negative control "one `verdict.json` in three directories does not reach the cap".

### 6. The default `--home` is the hinge of 49-11's close, and no gate pins it (the runbook is forbidden to pin it explicitly)

The only statement of the default is prose in a contract file — `$P/49-03-PLAN.md:81`: "`P49` is `$HOME/.cache/devflow-phase49` unless a tool is given `--home <dir>`" — and 49-03's gate checks five unrelated strings, not the path (`$P/49-03-PLAN.md:159`). 49-09's usage line (`$P/49-09-PLAN.md:76`) lists `[--home DIR]` with no default, and its real gate always passes one (`$P/49-09-PLAN.md:230`). The runbook's command is `bash "$T/launch-attempt.sh" prepare --attempt "$N"` with no `--home` (`$P/49-10-PLAN.md:117`), while 49-10's gate forbids any literal home path in the runbook (`literal_home_paths` must be 0, `$P/49-10-PLAN.md:159`) and its required-text list never mentions `devflow-phase49`. Two consumers hardcode it: `$P/49-11-PLAN.md:106` (`P="$HOME/.cache/devflow-phase49"`) and `$P/49-12-PLAN.md:132` (`H="$HOME/.cache/devflow-phase49"`).
Impact: if the implemented default lands anywhere else — the natural slip is `$HOME/.cache/devflow`, DevFlow's own root (`crates/devflow-core/src/registry.rs:61,66`: cache `HOME/.cache/devflow`, registrations under `roots/`; the host's `~/.cache/devflow/roots` is a live 700 KB directory) — the operator's attempt evidence lands in DevFlow's registry tree, `--attempt-gate` reads an empty directory forever, and `cap_reached` is unreachable too because no attempts are counted. The phase's single checkpoint can then never close, in either direction.
Recommendation: add a gate case that runs `prepare --attempt 1` with `HOME` redirected and *without* `--home`, asserting `$HOME/.cache/devflow-phase49/current-attempt`; or require the runbook to pass `--home "$P49"` on every tool line.

### 7. An autonomous plan (`49-07`, `autonomous: true`) mutates the operator's real DevFlow registry and builds in the reviewed checkout

`$P/49-07-PLAN.md:187-193` (the plan's own admission): "the unit's `HOME` is the real one, so the run registers the temp repository in DevFlow's machine registry under `~/.cache/devflow/roots/` … On a failure path the entry is left naming a path the trap has deleted … Neither negctl nor this gate deletes registry files". The gate itself (`$P/49-07-PLAN.md:197`) runs, inside an autonomous task gate:
```
cargo build -p devflow > "$L/build.log" 2>&1 … network? no; B="$PWD/target/debug/devflow" … rn=$(rg -l -F "$L/" "$HOME/.cache/devflow/roots" …) ; echo "registry_entries_naming_run=$rn (measurement, not gated)"
```
and it launches a **real** `devflow start --mode auto` through a real systemd user unit (`--unit p49-plan07-negctl`). `cargo build -p devflow` is not `--locked`, although 49-09's `prepare` for the same binary is (`$P/49-09-PLAN.md:76`, must_haves: "`cargo build -p devflow --locked`").
Impact: an autonomous plan writes to the shared `target/` of the working checkout, can rewrite a tracked `Cargo.lock`, and on its failure path knowingly leaves a stale registration in the operator's live registry (726 KB of existing entries) with cleanup delegated to a machine-wide `gate sweep` the plan refuses to run. The real negative arm also spends a live DevFlow run (bounded at 300 s) and, if the run reaches a preflight Define gate later than the bound, `negctl_exit=124` fails the gate.
Recommendation: use `cargo build -p devflow --locked --target-dir "$L/target"`; run the arm with `XDG_CACHE_HOME="$L/cache"` (or an equivalent isolated cache) so `registry.rs:48` never touches the real `~/.cache/devflow`; and make `registry_entries_naming_run` a gated 0 rather than a print.

### 8. 49-10's content instruction contradicts 49-10's own two-tool gate

`$P/49-10-PLAN.md:117`: "- 2: `bash \"$T/negctl.sh\" --attempt \"$N\"` then `bash \"$T/check-evidence.sh\" --arm negative \"$EV\"`; a FAIL here is a stop before any quota is spent." Item 7 (`$P/49-10-PLAN.md:151`) likewise names `collect-attempt.sh` and `check-evidence.sh` on one line. The same task's gate computes and requires zero (`$P/49-10-PLAN.md:159`):
```
mt=$(rg -n -o -e "launch-attempt\.sh" -e "negctl\.sh" -e "collect-attempt\.sh" -e "check-evidence\.sh" "$R" | sort -u | cut -d: -f1 | uniq -d | wc -l); … [ "$mt" -eq 0 ] || rc=1
```
Impact: an executor transcribing the bullet verbatim emits a runbook line whose two tool names collapse to one line number → `mt=1` → both 49-10 gates fail on correct work; the failure is reported as a runbook defect, not as a plan defect.
Recommendation: split the bullets onto separate lines (the action already says "one tool invocation per line"), or scope the two-tool check to fenced `bash` blocks only.

## SUSPECTED

- **R-1's stop/resume model (attack 3).** I checked the halt machinery in the installed GSD 1.14.0 (`~/.claude/gsd-core/bin/lib/plan-dependency-graph.cjs:126-129`, `lib/phase.cjs:952-956`): `halted` comes from a SUMMARY's frontmatter `status`, and "a designed stop still writes a completion record (#2830's model — its dependents get `blocked_by` halt propagation)". A 49-11 with *no* SUMMARY therefore does not mark 49-12 `blocked_by`, so `execute-phase.md:355-357`'s "Phase stuck … resolve the halt" branch should not fire. I could not verify the decisive step: `execute-phase/steps/completion-reconciliation.md` (referenced at `execute-phase.md:777` for abnormal session ends) is not in the snapshot, so if an interrupted checkpoint ever writes a `49-11-SUMMARY.md`, `identify_plan` ("first PLAN without matching SUMMARY", `execute-phase.md:83-85`) would skip the checkpoint forever and 49-12 would judge unclosed evidence. Also unverified: the *installed* tree is not the copy under review (`_external/`), and I read it read-only outside the snapshot — treat as strong indication, not proof.
- **The fix-vehicle loop vs the `(49-11)` commit scope.** `$P/49-11-PLAN.md:106` fails on `plan_commits_before_summary` via `^[a-z]+\(49-11\)`, and GSD's own `safe_resume_gate` uses the same anchored scope (`execute-phase.md:196-201`). The runbook's fix vehicle section (`$P/49-10-PLAN.md:130-142`) names `/gsd-quick` and `/gsd-phase --insert 49` but never forbids a `(49-11)`-scoped commit; a quick fix committed as `fix(49-11): …` would both fail the verify and trip GSD's illegal-partial-state recovery on the next resume. I did not verify what scope `/gsd-quick` writes.
- **49-07's "preflight refuses at Define before any agent launch"** (`$P/49-07-PLAN.md:190`) versus its own checker, which requires a Define gate snapshot *and* `worktree-head.sha == develop.sha` (`$P/49-07-PLAN.md:135-136`). I could not establish from `crates/devflow-cli/src/pipeline_launch.rs` whether the Define gate that fires here is the preflight-failure gate (pre-agent) or the post-Define-stage gate (post-agent); the selftest's own fake "adds a worktree on `feature/phase-01` from `develop`" at the gate, which is consistent with either. If it is post-agent, 49-07's Task 3 spends real Claude quota inside an autonomous gate.
- **49-11's `--attempt-gate` vs `49-08`'s spec.** 49-11 calls `check-evidence.sh --attempt-gate "$P"` and greps `^ATTEMPT_GATE verdict=([a-z_]+)`; 49-08's spec fixes the last line to exactly `ATTEMPT_GATE verdict=shipped attempt=<N>` / `…cap_reached stopped_attempts=<K>` / `…not_ready attempt=<N> reason=<text>` — consistent, but the two plans never agree on stdout vs stderr for `not_ready`'s `reason=`; 49-11 captures `2>&1`, so it is fine today and only fragile.

## CHECKED AND FINE

- All 29 `<automated>` gates (`$P/49-01-PLAN.md:160` … `$P/49-12-PLAN.md:206`) are syntactically valid bash and contain no unescaped single quote: I extracted each `bash -c '…'` body and ran `bash -n` on it — 29/29 OK. The backtick quoting in `49-01-PLAN.md:160` (`\`devflow.toml\``) is correctly escaped for the outer parse.
- The `cargo test --exact` trap is defended. `$P/49-06-PLAN.md:366` filters `gsd_config::tests::writing_the_flag_leaves_every_other_key_byte_identical -- --exact`; that test exists at `crates/devflow-core/src/gsd_config.rs:783` inside `mod tests` (`:438`), so the full path matches, and the gate rejects a zero-match run with `ok=$(grep -c "test result: ok. 1 passed")` `-eq 1`.
- `rg -c`/`--count-matches` zero-match handling is correct everywhere it is compared: each assignment carries `|| true` and is read as `${x:-0}` (e.g. `$P/49-07-PLAN.md:159`, `$P/49-12-PLAN.md:206`).
- `$P/49-02-PLAN.md:223`'s hardcoded develop tree hash is right: I reproduced the scaffold in a scratch repo and `git rev-parse develop^{tree}` printed exactly `f6120b337cb9b37d658c6111b7ac14694338dc56` (the destination tree is content-only, so it is stable across hosts). The "default mode: tree hash unchanged" case and the separate hash assertion therefore agree.
- `$P/49-12-PLAN.md:173`'s `jq '.phase == 1'` type is correct: an integer phase serializes as a bare JSON number (`crates/devflow-core/src/phase_id.rs:59-72`) and the envelope carries it (`crates/devflow-core/src/events.rs:36-42`).
- `advance_failed` is a real event (`crates/devflow-cli/src/pipeline_launch.rs:1468-1473`), so requiring it in the SURV-01 section is not naming a fiction.
- 49-01's before/after texts match the files: `REQUIREMENTS.md:117` (`**Carries a setup step:** this repository has no committed \`devflow.toml\`.`), traceability row `REQUIREMENTS.md:202`, criterion 2 `ROADMAP.md:264-267`, `49.1` at `ROADMAP.md:319,324`, `landing hop` at `:297`; `### Phase 50:` exists at `ROADMAP.md:329`, so the `sed` range is 81 lines (≥40) and contains exactly 5 criteria; `### Phase 999.55:` exists at `ROADMAP.md:4087` for 49-01's Task 3 gate.
- 49-10's checklist anchor exists verbatim: `.planning/user/DEV-SETUP-CHECKLIST.md:50` contains ``- [ ] **[GLOBAL]** `gh auth status` — …``.
- 49-05 Task 3's host precondition holds: `/var/home/denniyahh/Github/devflow/.devflow/history/phase-43` has 12 `*-stdout` captures (≥ the required 4).
- Selftest-count arithmetic across chained gates is coherent: 49-02 ≥4/≥10 → 49-06 ≥14/≥20/≥23; 49-07 ≥1/≥7 → 49-08 ≥8/≥17/≥25; 49-03 (two tools) ≥6/≥11; 49-04 ≥2/≥7; 49-05 ≥4/≥12/≥15; 49-09 ≥3/≥16.
- `$P/49-11-PLAN.md:106`'s `plan_commits_before_summary` check is not self-defeating: the SUMMARY commit subject does match its regex (`_external/gsd-core/workflows/execute-plan.md:539`: `docs({phase}-{plan}): complete [plan-name] plan`), but `create_summary` (`:406`) follows `checkpoint_protocol` (`:337`) and `verification_failure_gate` (`:354`) in the workflow, so the check runs before that commit exists. (Ordering inferred from step order, not from an execution trace.)

Decision needed before this plan set is executed: which of findings 1-8 you want patched into the plans (my recommendation: 1, 2, 4, 5, 6 are the ones that change an outcome — 1 and 2 corrupt the criteria verdicts, 4 can consume the only human gate, 5 can close the phase without a run, 6 can wedge it permanently).
