I read the phase artifacts and all 32 plans, and checked their claims against `crates/` and the host toolchain. Findings below; everything marked CONFIRMED was reproduced against the tree at this snapshot.

## CONFIRMED

**F1 — Critical — `52-32-PLAN.md` Task 1: the launch-count assertion is arithmetically impossible for the fixture it names**
Quoted (`52-32-PLAN.md`, Task 1 behaviour): "`rig.invocations("claude")` is exactly 2 after the second denial (one original, one automatic relaunch) and stays 2 over a bounded wait".
Evidence in source: `crates/devflow-cli/tests/failover_e2e.rs:314-331` — `Rig::start_with` always passes `--mode auto --until code`, so one run logs Define, Plan and Code turns; `:390-398` — `invocations()` filters only `"health"`, **not** `"canary"`; `:471-475` — the existing test on this exact rig asserts
`rig.invocations("claude")` == `["canary", "stage define", "stage plan", "stage code"]` (4 entries).
So one automatic Code relaunch yields 5 entries, never 2, and "stays 2" can never hold. Why it breaks: the gate `a_passed_reset_relaunches_once_and_a_second_denial_waits` is the phase's only proof of CUT-03's "exactly one automatic relaunch" (ROADMAP criterion 3). An executor either games the count (filters to `stage code` and asserts 2 — which counts Define/Plan/Code, not relaunches) or the test cannot be satisfied.
Fix: assert on relaunches, not invocations — count `stage_launched` events for stage `code`, or `invocations("claude").iter().filter(|l| *l == "stage code").count() == 2`, with the pre-relaunch count asserted first as the control.

**F2 — Critical — `52-32-PLAN.md` context names a constant that does not exist, and the work it implies is not planned**
Quoted: "`CLAUDE_STUB` and its `@CLAUDE_RATE_LIMIT@` substitution (~L66-99, ~L275), `RESETS_AT` constant (~L45, already past)".
Evidence: `rg 'RESETS_AT' crates/devflow-cli/tests/failover_e2e.rs` → **no match**. The reset value is inlined inside the JSON fixture: `failover_e2e.rs:45` `const CLAUDE_REJECTED_RATE_LIMIT_EVENT: &str = …"resetsAt":1785645600,…`, substituted once at `:275`. Task 1 requires "the claude stub denies twice with a computed past `resetsAt`" and Pitfall 10 requires "e2e reset times are computed from the current time". Both need a new `@RESETS_AT@` placeholder threaded through `CLAUDE_STUB` and `Rig::build` (two separate code points); nothing in the plan asks for it, and `files_modified` cannot express it.
Fix: add the placeholder explicitly in Task 1's action and to the anchors, and say which two literals change.

**F3 — High — `52-19-PLAN.md` removes note-substring abort but leaves an e2e that depends on it, outside `files_modified`**
Quoted (`52-19-PLAN.md` truth): "`gate reject` always means fix and relaunch"; `files_modified` lists `gate_wedge_e2e.rs`, `stop_e2e.rs`, `start_lock_e2e.rs`, `recover_clean_e2e.rs` — **not** `phase7_cli.rs`.
Evidence: `crates/devflow-cli/tests/phase7_cli.rs:912-917` — "…abort it (note containing \"abort\" — `gates.rs::GateAction::from_response`) so the monitor clears state"; `:932-949` — `gate reject 8 --stage validate --note "abort test teardown"` then `wait_for_state_cleared(root, PhaseId::new(8))`.
After 52-19 that reject loops back; the state is never cleared and the fixture blocks until its wait bound. 52-19 Task 3's gate is `scripts/check.sh all`, so the break lands unplanned and the executor must edit a file the plan does not authorise (which is also how `files_modified` drift starts).
Fix: add `crates/devflow-cli/tests/phase7_cli.rs` to 52-19 `files_modified`, change that teardown to `gate abort 8 --stage validate`, and list it with the other writers.

**F4 — High — `52-30-PLAN.md` names a cap in bytes and tests it against a cut that counts characters**
Quoted (must_haves): "bounded by a stored cap of 4096 **bytes**, `GATE_CONTEXT_STORED_MAX_BYTES`"; behaviour: "exactly 4096 **bytes** → unchanged; 4097 **bytes** with a multi-byte character at the boundary → cut before it".
Evidence: `crates/devflow-cli/src/pipeline_outcomes.rs:893-914` — `pub(crate) fn render_gate_context(context: &str, max_chars: usize)` and `if sanitized.chars().count() <= max_chars { return sanitized; }`. A 4097-byte string containing a 3-byte character is 4095–4096 **chars**, so it is returned unchanged and the named test `a_context_at_the_stored_cap_is_unchanged_and_one_over_is_cut_at_a_char_boundary` fails exactly as specified. The constant is also the artifact 52-30 promises (`contains: "GATE_CONTEXT_STORED_MAX_BYTES"`), so the name misstates the unit for every later reader.
Fix: rename to `…_MAX_CHARS` and state the count in chars, or implement a byte-bounded cut (and then the "cut at a char boundary" phrasing needs the truncation suffix to fit inside the byte budget).

**F5 — High — `52-18-PLAN.md` excludes `CHANGELOG.md` from a criterion that says "the top-level docs"**
Quoted (`52-18-PLAN.md`): "`CHANGELOG.md` and `.planning/` are out of scope"; `52-ROADMAP.md` criterion 2: "…remains under `crates/`, `docs/` or **the top-level docs**, comments stripped"; `REQUIREMENTS.md` CUT-03: "**nothing in the tree or docs refers to it**".
`CHANGELOG.md` is a top-level doc and, by 52-18's own reasoning, will name removed surface (it must record the removal). The plan records this only as an "Interpretation … in the SUMMARY so verification carries it" — a verifier checking criterion 2 literally fails the phase. The precedent for fixing the criterion text rather than the scan exists: D-17 amended CUT-03/ROADMAP criterion 3 for a different reason and the amendment is recorded in CONTEXT.
Fix: amend REQUIREMENTS CUT-03 and ROADMAP criterion 2 to exclude release history, in the same way D-17 amended criterion 3 — or get an explicit operator answer that the interpretation stands (this is a decision point, not a planner choice).

**F6 — High — "approve to ship" survives past 52-18's "cut complete" checkpoint and no gate in the phase can see it**
Quoted: `crates/devflow-cli/src/pipeline_outcomes.rs:729` —
`" The per-phase ceiling of {} is reached: this run is paused for a human, not aborted — approve to ship, reject to loop back for another pass, or abort."`
52-22 explicitly defers it: "Any sentence that tells the operator what to answer … is interim: 52-29 replaces … so add no new answer wording here". So from 52-15 (no `Stage::Ship`, no Ship prompt) to 52-29, the failure-cap gate tells the operator to approve a Ship that no longer exists — including across 52-18, whose Task 3 is the phase's nominal "cut tree is green / semver matches the plan" checkpoint.
Why no gate catches it: 52-18's token list matches identifier-bounded identifiers plus the whole string literals `"ship"`/`"define"`; this sentence contains neither form, and `help_names_no_removed_surface` only scans three `--help` outputs (52-15/52-17).
Fix: add the phrase `approve to ship` (and `loop back for another pass`) to 52-18's token list, or reword the sentence when 52-22/52-27 touch that gate.

**F7 — Medium — `52-18-PLAN.md`'s docs-site guard cannot police the diagram it claims to**
Quoted (must_haves): "fails while … `docs/diagrams/module-map.md` names a module that `crates/devflow-core/src/lib.rs` does not declare"; rule (3) in Task 2: "every backticked name in the table's Modules column and every mermaid node label **that is a single lowercase identifier**".
Evidence `docs/diagrams/module-map.md:6-20`: the mermaid nodes are `CLI[devflow-cli]`, `WF[workflow and state]`, `HOOKS[hooks]`, `VERSION[version]`, `LOCK[lock]`, `SHIP`… — all labels are **uppercase**, so no node matches "a single lowercase identifier" and none is ever checked. Only the table rows (`| Git lifecycle | … \`hooks\`, \`version\`, \`ship\` |`) can fail. The stated invariant is strictly broader than the mechanism; after 52-12/52-13 delete those modules, the diagram still draws them and the test stays green.
Fix: match the payload inside each label (`\[(.*?)\]`) or the node id against `lib.rs`, or narrow the truth to the table.

**F8 — Medium — two durable carriers with different lifetimes for the same operator note**
Quoted (`52-31-PLAN.md` Task 4, action): "a Code pass that already carries a `code_fix_note` shows **both** blocks, its own first"; 52-26 clears `code_fix_note` "when the fix pass completes"; 52-31 clears `relaunch_note` "when the pass it was given to ends, whatever the outcome".
Both are `State` fields on the same stage (`crates/devflow-core/src/state.rs` shape, 52-26 `code_fix_note`, 52-31 `relaunch_note`). A rate-limit reject that lands during a Code fix pass therefore hands the relaunched agent the previous rejection's note *and* the new one, with nothing distinguishing which pass each belongs to — and the byte-identity tests in 52-26/52-31 pass either way, because each asserts only its own block.
Fix: one carrier (`relaunch_note`) written by both reject paths, or tag each block with the gate/stage it came from and clear on the same event in both plans.

**F9 — Medium — `52-23-PLAN.md`'s never-silent audit is unbounded and verified by judgement only**
Quoted (must_haves): "Never-silent audit: every `warn!`/`error!` reachable from a foreground command that is the only operator-visible signal … also prints a plain stderr line (or is already backed by a gate or event); the SUMMARY lists each site and its disposition", backed by a `prohibitions` entry with `verification: judgment`. The plan's own context sizes the job: "About 94 `warn!`/`error!` sites exist across both crates (`monitor.rs` 23, `pipeline_launch.rs` 12, `config.rs` 10, `gates.rs` 9, `supervisor/runtime.rs` 9, …)".
OPS-03 asks only that "command output carries no `tracing` lines unless `RUST_LOG` is set". Turning the default filter to `off` (`EnvFilter::new("off")` in both branches of `main.rs`) turns 94 diagnostics off by default, and the mitigation is a 55k-token plan with no gate: nothing in `<automated>` can fail if the audit is done badly. The highest-risk sites (`monitor.rs`, detached `__monitor` with stderr at `/dev/null`) are also the ones the plan excludes by fiat.
Fix: bound it — enumerate the sites that are *the only* signal (the ones 52-30/52-08 already gate on), give each a named test that asserts the stderr line, and scope the audit claim to that list.

**F10 — Low — `52-16` leaves `--until define` accepted and unreachable, which is the exact class of bug D-07 exists to prevent**
Quoted (52-16 must_haves): "`Stage::Define` still compiles but no code path starts or transitions into it"; 52-16 Task 1 sets `State::new` to `Stage::Plan` and `print_dry_run` to start at Plan. `52-17` then makes `--until define` a clap error ("D-07: `devflow start --until define` is rejected by clap").
In between, `--until define` parses (`Stage::Define` exists, `crates/devflow-core/src/stage.rs`), no run can ever reach Define, and `until_rejects_removed_stages` (created in 52-15) does not cover `define` yet — so the CLI advertises a `--until` value that silently runs the whole pipeline. `--dry-run`'s annotation (`start_dry_run_annotates_until_stage`, rewritten in 52-16) will also produce a marker on a stage line that does not exist.
Fix: reject `define` in the same plan that makes it unreachable (a one-line guard on the `--until` arg), or state the window explicitly and test that `--until define --dry-run` fails.

---

## SUSPECTED (speculation — flagged as such)

**S1 — Medium — `52-20-PLAN.md` `phase_dir` returns an arbitrary directory on a prefix match**
Quoted: "`gsd_reader::phase_dir(evidence_root, phase)` returns the phase DIRECTORY found by the `{padded}-` prefix scan under `.planning/phases`".
The existing analogue it is modelled on requires a discriminator: `crates/devflow-core/src/agent_result.rs:3817-3838` (`phase_verification_path`) only accepts a directory that *contains* `{padded}-VERIFICATION.md`, and 52-20 deliberately drops that requirement so `missing` is reachable. The scan then returns the first `read_dir` entry whose name starts with `{padded}-`; for phase 52 in this repo the phase-dir subtree already contains sibling artifact directories named `52-context-review/` and `52-gap-loop-evidence/` (they sit inside it, not beside it — which is why I am marking this suspected rather than confirmed), and nothing in `gsd_reader` forbids a project from putting such a directory at `.planning/phases/`. If that happens, `phase_dir` returns a directory GSD does not recognise and D-24 converts it to a never-silent gate — a permanently wedged Validate.
Fix: accept a candidate only if it contains the phase's own artifacts (`{padded}-*-PLAN.md` or `{padded}-VERIFICATION.md`), falling back to a single prefix match, and test a two-candidate directory.

**S2 — Low — `52-30` silently changes preflight refusal text outside GATE-04**
Quoted (52-30 Task 1): "switch every write-time caller to the stored cap", with `crates/devflow-cli/src/preflight.rs` in `files_modified`. `truncate_reason` is the shared helper (`pipeline_outcomes.rs:888`), and `preflight.rs:919-922` documents a deliberate 300-character cap for preflight refusals ("a hard 300-character cap …"). Raising it to 4096 puts up to 13× more agent-derived text into preflight refusals. Nothing in 52-30 tests the preflight path, and `999.206` is about gate contexts only. I have not established that 4096-char preflight refusals are harmful — only that the change is untested and undocumented in the plan.

**S3 — Low — `52-04`'s fake `codex` may not survive the codex driver's preflight**
Quoted: "answer any probe arguments the preflight sends (for example a version or login query) with a success line". The witness must satisfy the delivery canary (`phase7_cli.rs:836-856` shows the canary is a *top-level* `result` event carrying a planted token) **and** the codex capture parser, in `sh`. The plan postpones the details to execution ("read `crates/devflow-core/src/agents/codex.rs` … and any codex preflight probe"). If the codex probe does not have a cheap argv-only success shape, the plan's "test-only plan" fails at its own tracer and the capture-shape coverage 52-05 depends on is lost.

---

## COVERAGE

- **Read in full** (every task, gate, truth): 52-01, 52-02, 52-03, 52-04, 52-05, 52-06, 52-07, 52-08, 52-09, 52-11, 52-12, 52-13, 52-14, 52-15, 52-16, 52-17, 52-18, 52-19, 52-20, 52-21, 52-22, 52-24, 52-25, 52-26, 52-27, 52-28, 52-29, 52-30, 52-31, 52-32. Plus 52-CONTEXT, 52-VALIDATION, ROADMAP Phase 52, REQUIREMENTS (CUT/GATE/OPS), CLAUDE.md, and 52-RESEARCH/52-PATTERNS only at the sections the plans cite.
- **Skimmed**: 52-10 (must_haves, anchors, actions, and both `<automated>` gates via targeted extraction — I did not read its threat model or full prose), 52-23 (Tasks 1–2 in full; the last ~86 lines, Tasks 3 and the closing sections, not read).
- **Checked against source and passing** (i.e. I tried to break these and could not): every `cargo nextest … -E 'test(=…)'` name that already exists (`doc_check::*`, `hermetic_spawn_sites`, `supervisor_e2e`, `phase7_cli`, `workspace_version_pin`, `build_provenance`, `log_format_env`, `gate_wedge_e2e` helpers, `parallel.rs` tests being moved); the `gates.rs`/`commands.rs`/`pipeline_outcomes.rs`/`mode.rs`/`agent_result.rs`/`prompt.rs`/`state.rs` line anchors I spot-checked (~20 of them, all within a few lines); `registry::{entry_path_in,roots_dir_in,deregister}`, `State::{stopped,stop_reason,fallback_consumed,new}`, `Cleanup{project,force}` positional, `gate_sweep_may_reap`, `git::publish_order`, `deny.toml`, `scripts/check.sh` using `--workspace`, `check-release --release-type`; `gsd_run` present in all three runtime homes and `query verification fingerprint` / `query frontmatter get --field` / `runtime-identity` all real in gsd-core 1.20.1. DAG levels for all 29 waves are consistent with `depends_on`, and no same-wave pair shares a file.
- **Not verified**: anything needing network or a live agent (crates.io baseline for `cargo semver-checks`, the GSD live routes), and 52-10/52-23's unread prose.
