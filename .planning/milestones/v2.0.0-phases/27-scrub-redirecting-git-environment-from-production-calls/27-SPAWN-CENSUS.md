# Phase 27 — Workspace Spawn-Edge Census

**Purpose:** Close (or explicitly escalate) `27-RESEARCH.md` Assumption A2 — the
exhaustiveness gap. A literal `Command::new("git")` grep is a floor, not a
ceiling, for "places a redirecting environment variable can reach `git`." This
document sweeps the whole merged tree (waves 1–2, everything through `27-05`)
for every production spawn edge, classifies each by whether it can reach
`git`, and states plainly whether A2 is closed.

**Measured against:** this worktree's HEAD (base `f539012f8656d37e41627c6015cf9bc4db509051`,
everything from `27-01`…`27-05` merged in).

---

## Sweep A — direct git constructions (comment-filtered)

Command run verbatim:

```
$ rg --no-heading -n 'Command::new\("git"\)' crates/devflow-core/src crates/devflow-cli/src
```

Output (7 hits, all doc/inline comments referencing the literal string, not code):

```
crates/devflow-cli/src/preflight.rs:2139:    /// pre-migration `ls-tree` step — `std::process::Command::new("git")`
crates/devflow-cli/src/preflight.rs:2159:    /// shell is inherited by every unscrubbed `Command::new("git")` this
crates/devflow-cli/src/preflight.rs:2179:        // `Command::new("git")` spelling, so this deliberately-unscrubbed
crates/devflow-core/src/version.rs:2422:    /// this plan's migration, both bare `Command::new("git")` sites this
crates/devflow-core/src/agent_result.rs:2531:    /// migration, the two unmigrated `Command::new("git")` sites inherit
crates/devflow-core/src/git.rs:54:/// `Command::new("git")` directly. `GIT_EXEC_PATH` is deliberately left
crates/devflow-core/src/git.rs:1710:    /// unscrubbed `Command::new("git")` reproduction chaining a hostile
```

Comment-filter command and result:

```
$ rg --no-heading -n 'Command::new\("git"\)' crates/devflow-core/src crates/devflow-cli/src | rg -v ':\s*(//|///|//!)' | wc -l
0
```

**Sweep A result: 0.** The 41-site literal-git migration is confirmed complete
by measurement, comment-filtered so a doc comment cannot mask a real site.

---

## Sweep B — every production `Command::new(...)` construction, any program

Command run verbatim (unfiltered, whatever the program name):

```
$ rg --no-heading -n 'Command::new\(' crates/devflow-core/src crates/devflow-cli/src
```

Output (43 hits total across both crates — see full list below, classified).
Each hit was checked against its file's `#[cfg(test)] mod tests { ... }`
boundary (confirmed by direct `wc -l` + `rg` to verify the test module
genuinely extends to end-of-file in every file below, not closed early):

| File | `#[cfg(test)] mod tests` starts at | File length | Boundary confirmed to EOF? |
|---|---|---|---|
| `devflow-core/src/git.rs` | 988 | 2072 | yes |
| `devflow-core/src/agent.rs` | 532 | 972 | yes |
| `devflow-core/src/monitor.rs` | 191 | 502 | yes |
| `devflow-core/src/hooks.rs` | 357 | 809 | yes |
| `devflow-core/src/gates.rs` | 366 | 647 | yes |
| `devflow-core/src/verify.rs` | 115 | 205 | yes |
| `devflow-core/src/version.rs` | 1097 | — | yes (no production hits below this file) |
| `devflow-core/src/agent_result.rs` | 1113 | — | yes (no production hits below this file) |
| `devflow-cli/src/staleness.rs` | 386 | 2131 | yes |
| `devflow-cli/src/preflight.rs` | 958 | 2753 | yes |
| `devflow-cli/src/commands.rs` | 3222 | 5937 | yes |

Two additional files (`devflow-core/src/test_support.rs`,
`devflow-cli/src/test_support.rs`) are **entirely** gated out of production
builds at the `mod` declaration itself — `devflow-core/src/lib.rs:78`
(`#[cfg(any(test, feature = "test-support"))] pub mod test_support;`) and
`devflow-cli/src/main.rs:7` (`#[cfg(test)] mod test_support;`) — so every
`Command::new(...)` inside either file is excluded from this census regardless
of its position relative to any inner `#[cfg(test)]` marker: neither file is
ever compiled into a shipped `devflow` binary.

### Full classification table

One row per production spawn edge (test-only and constructor-mechanism rows
included for completeness, marked accordingly).

| File:Line | Program | Classification | Disposition | Rationale |
|---|---|---|---|---|
| `git.rs:77` | parameterized (`hermetic_command`'s own `program` argument) | mechanism, not a call site | n/a | This is the scrub constructor itself — the `env_remove` loop lives here. Every caller that routes through it is scrubbed by construction. |
| `git.rs:777` | `ssh-keygen` | cannot-reach-git | safe, no action | Standalone SSH key generation for signing-viability checks; no git subprocess spawned. |
| `git.rs:831` | `ssh-keygen` | cannot-reach-git | safe, no action | Second `ssh-keygen` call site, same rationale. |
| `git.rs:890` | `ssh-add` | cannot-reach-git | safe, no action | `ssh-add -l` lists agent-loaded keys; no git involvement. |
| `git.rs:944` | `gpg` | cannot-reach-git | safe, no action | GPG signing-viability probe, standalone program. |
| `hooks.rs:222` (`docs_update`) | `sh -c "cargo doc --no-deps"` | **reaches-git** | **UNMITIGATED** | `cargo doc` compiles the crate graph, which runs `devflow-cli`'s own `build.rs::run_git` at compile time to embed `DEVFLOW_BUILD_COMMIT` — the *same* indirect chain `27-04` closed for `commands.rs::test_cmd`, but this site was not in `27-04`'s scope and remains unscrubbed. Confirmed production-reachable: dispatched via `Hook::DocsUpdate => docs_update(ctx)` (`hooks.rs:81`). |
| `hooks.rs:341` (`today`) | `date` | cannot-reach-git | safe, no action | `date +%Y-%m-%d`, no subprocess chain of any kind. |
| `gates.rs:323` (`run_notify_command`) | `sh -c <DEVFLOW_GATE_NOTIFY_CMD>` | **reaches-git** | **UNMITIGATED** | The command string is fully operator-supplied via an env var the operator sets (`fire_gate_notify`); `sh -c` with an arbitrary string can invoke `git` or anything else. Confirmed production-reachable: called from `pipeline_gate.rs:325` and `staleness.rs:350`, both outside any test module. |
| `verify.rs:106` (`run_external_verification`) | `sh -c <cmd>` | **reaches-git** | **UNMITIGATED** | Same class as `gates.rs`: an explicitly operator-approved external verification command, "intentional because probes may contain pipelines" per its own doc comment. Confirmed production-reachable: called from `agent_result.rs:769`, outside any test module. |
| `monitor.rs:148` (`spawn_monitor_inner`) | `sh -c <script>` which then execs `<program> <args>` | **reaches-git** | **UNMITIGATED — highest consequence** | The `program`/`args` pair is the AI agent binary (`claude`/`codex`/etc.) that performs the phase's actual git work — commits, pushes, merges. A hostile `GIT_DIR` inherited by the parent `devflow` process reaches every git operation the agent itself performs, two-plus processes down. Confirmed production-reachable: called from `pipeline_launch.rs:123`, outside any test module. This is the site the plan's own read-first material named as the highest-consequence candidate. |
| `preflight.rs:608` (`preflight_gh_auth_check`) | `gh auth status` | cannot-reach-git | safe, no action | Fixed argv (`auth`, `status`); the GitHub CLI's auth-status subcommand makes an API call and does not shell to `git` for that operation. Re-evaluate if this call site's argv ever changes to a git-context-aware `gh` subcommand (e.g. `gh pr checkout`). |
| `commands.rs:1999` (`cmd_check`, called with `cmd="git"` at `commands.rs:2086`) | `git` (passed as a runtime `&str`, not a literal in the `Command::new(...)` call) | **direct-git** | **UNMITIGATED, and invisible to Sweep A's literal grep** | `devflow doctor`'s environment check runs `git --version` unscrubbed — a genuine, if functionally inert, literal git invocation D-01's unconditional wording covers ("every production git call"). It evades Sweep A because the program name is threaded through a generic `cmd_check(name, cmd, version_arg, install_hint)` helper as a variable, never spelled `Command::new("git")` in source. Functionally low-risk: `--version` performs no ref/object/worktree resolution, so a hostile `GIT_DIR` cannot change its output — but it is a real, previously-uncounted site (not one of CONTEXT.md's 3 listed `commands.rs` lines: 91, 2886, 2892). |
| `commands.rs:1999` (`cmd_check`, other callers: `cargo`, `gh`, `claude`, `codex`, `opencode`, each with `--version`) | various | cannot-reach-git | safe, no action | Plain `--version` queries only; none trigger a build or repo-context operation. |
| `commands.rs` (`test_cmd`, `hermetic_command("sh", project_root)` call, ~line 1955) | `sh -c "cargo test / cargo clippy / cargo fmt --check"` | reaches-git | **MITIGATED (27-04)** | Already routes through `hermetic_command`, closing the `sh → cargo → build.rs::run_git` chain at DevFlow's own spawn edge. Verified live: `rg -n 'hermetic_command\("sh", project_root\)' crates/devflow-cli/src/commands.rs` → 1 hit at line 1955. |
| `agent.rs:707` | `sh -c "trap '' TERM; sleep 30"` | test-only | n/a | Inside `#[test] fn terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child`, below the `#[cfg(test)] mod tests` boundary at line 532 (confirmed, not assumed — file is 972 lines, module extends to EOF). |
| `agent.rs:739` | `sh -c "trap cleanup TERM INT; sleep 30"` | test-only | n/a | Inside `#[test] fn discover_stray_devflow_processes_finds_a_monitor_wrapper`, same boundary confirmation as above. |
| `agent.rs:778` | `sh -c "sleep 30" <path>` | test-only | n/a | Inside `#[test] fn discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`, same boundary confirmation as above. |
| `agent.rs:548,613,677,818,947` | `true` / `sleep` | test-only | n/a | Same module, all below the line-532 boundary; process-liveness test fixtures, no git relevance regardless. |
| `staleness.rs:1063` | `Command::new(&exe)` (re-invokes the test binary itself) | test-only | n/a | The spawned-child-test-process hostile-`GIT_DIR` injection technique `27-04` introduced (`embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`), below the `#[cfg(test)]` boundary at line 386. |
| `preflight.rs:2184,2592` | `Command::new(git_program)` (`git_program` a local `let` bound to `"git"`, deliberately not the literal grep spelling) | test-only | n/a | The manually-reproduced pre-migration vulnerable shape from `27-05`'s two regression tests, both below the `#[cfg(test)]` boundary at line 958 — by design, so these deliberately-unscrubbed TEST reproductions are never counted as unmigrated production sites. |
| `preflight.rs:2139,2159,2179` | n/a (comments) | test-only / comment | n/a | Doc comments inside the same test module, already excluded by Sweep A's comment filter and confirmed below the 958 boundary. |
| `commands.rs:3728,3758,3787,3820,3858,3899,3992,4038,4196,5135,5256,5752` | `sleep` / `sh` / `Command::new(&exe)` | test-only | n/a | All below the `#[cfg(test)] mod tests` boundary at line 3222 (file is 5937 lines, module extends to EOF) — process-liveness and gate-sweep test fixtures. |
| `devflow-core/src/test_support.rs:153` | `sleep` | test-only, file-level excluded | n/a | Whole module gated `#[cfg(any(test, feature = "test-support"))]` (`lib.rs:78`) — never compiled into the shipped binary regardless of position relative to any inner marker. |
| `devflow-cli/src/test_support.rs:454` | `sleep` | test-only, file-level excluded | n/a | Whole module gated `#[cfg(test)] mod test_support;` (`main.rs:7`) — never compiled into the shipped binary. |

### Sites explicitly named in the plan's "already known" table

| Site | Line (this HEAD) | Spawns | Verdict |
|---|---|---|---|
| `commands.rs::test_cmd` | ~1955 (`hermetic_command("sh", project_root)`) | `sh -c "cargo …"` | **MITIGATED by 27-04** — verified above, cited in `27-04-PLAN.md`'s recorded IN-SCOPE decision. |
| `monitor.rs` | 148 | `sh -c …` → agent binary | ~~**UNMITIGATED — highest-consequence candidate**~~ → **MITIGATED after this census, by `936b371` (review WR-03)**: routed through `hermetic_command`. Flagged here exactly as the plan's read-first material predicted, and closed on that signal. |
| `agent.rs` | 707, 739, 778 | `sh -c …` | **Confirmed test code**, below the `#[cfg(test)]` boundary at line 532 — verified by direct inspection, not assumed. |
| `hooks.rs` | 222 | `sh -c …` | **UNMITIGATED.** A git hook context can legitimately have `GIT_DIR` set by git itself when invoking the hook — but `docs_update`'s own spawn (`cargo doc`) is a *DevFlow-issued* child of that hook invocation, not git re-invoking itself, so the "git sets it legitimately" caveat does not exempt this specific site. This needs judgment (per the plan's own instruction), and the judgment call here is: reaches-git, unmitigated, out of this plan's scope to fix. |
| `gates.rs` | 323 | `sh -c …` | **UNMITIGATED.** |
| `verify.rs` | 106 | `sh -c …` | **UNMITIGATED.** |

---

## Assumption A2 verdict

**A2 is OPEN.**

> **Correction, 2026-07-30 (post-census, supersedes the count below).** Site 4
> (`monitor.rs:148`) was **closed after this census was written**, by the code
> review's WR-03 finding and commit `936b371` (`fix(27): close review
> WR-01/WR-02/WR-03`) — the agent spawn is now built through
> `hermetic_command`, with `.envs(...)` applied after construction so an
> adapter that deliberately sets one of these vars still wins. **Four** sites
> remain open, not five, and the highest-consequence one is no longer among
> them. The enumeration below is preserved as written for provenance; read it
> with this correction applied. The proposed backlog entry's title, evidence
> and severity paragraph are corrected in place.

Five unmitigated spawn edges remain that can reach `git`, none of them among
the 41 counted `Command::new("git")` sites and none in `27-CONTEXT.md`'s
7-file scope list:

1. `hooks.rs:222` (`docs_update`) — `sh -c "cargo doc --no-deps"`, same
   indirect `sh → cargo → build.rs::run_git` chain `27-04` closed for
   `test_cmd`, left open here.
2. `gates.rs:323` (`run_notify_command`) — `sh -c <operator-supplied
   DEVFLOW_GATE_NOTIFY_CMD>`.
3. `verify.rs:106` (`run_external_verification`) — `sh -c <operator-approved
   verification command>`.
4. ~~`monitor.rs:148` (`spawn_monitor_inner`) — `sh -c` that execs the AI agent
   binary itself, the single highest-consequence site: the agent performs the
   phase's actual git commits/pushes.~~ **CLOSED by `936b371` (review WR-03).**
   Now constructed via `hermetic_command`; no longer open.
5. `commands.rs:2086` (via `cmd_check`, `doctor`'s environment check) — a
   **direct-git** construction (`git --version`), invisible to a literal
   `Command::new("git")` grep because the program name is threaded through a
   variable. Functionally inert (no ref/object resolution occurs for
   `--version`), but a real, previously-uncounted, unscrubbed git invocation.

Per this plan's explicit scope boundary, **none of these five sites were
migrated.** Migrating them would require per-site judgment this wave-3
acceptance plan does not have the context budget for (missing information —
`hooks.rs` in particular is a real counter-example where git itself can
legitimately set `GIT_DIR` for a hook, so a reflexive scrub could be wrong)
and would expand this plan into a second migration rather than a
verification step.

### Proposed backlog entry

**Title:** Scrub the four remaining indirect/dynamic git-reaching spawn edges
the Phase 27 literal-grep migration could not see (`hooks.rs`, `gates.rs`,
`verify.rs`, `commands.rs::cmd_check`)

**Evidence:** This document, `27-SPAWN-CENSUS.md`, § Full classification
table and § Assumption A2 verdict — originally five sites with file, line,
program, and rationale, all confirmed production-reachable by direct
call-graph tracing (not assumed). The fifth, `monitor.rs:148`, was closed
in-phase by `936b371` and is **excluded** from this entry.

**Severity:** ~~high for `monitor.rs:148`~~ — **that site is closed; the
highest-severity item is no longer part of this entry**, which lowers the
whole entry's ceiling to Medium. Medium for `hooks.rs:222` (same indirect-compile mechanism 27-04 already
found and fixed once, just at a second call site). Medium for `gates.rs:323`
and `verify.rs:106` (both are operator-configured/approved commands, so the
operator already has some control over what runs, but the redirecting
variables are still inherited invisibly into a command whose contents the
operator did not audit for env-sensitivity). Low for `commands.rs:2086`
(functionally inert, `--version` has no ref-resolution surface, but worth
closing for D-01 literal-completeness and because it demonstrates a literal
grep is defeatable by any indirection through a variable).

---

## Reproduction commands (for a future closing plan)

```
# Comment-filtered literal-git sweep (Sweep A), for regression:
rg --no-heading -n 'Command::new\("git"\)' crates/devflow-core/src crates/devflow-cli/src | rg -v ':\s*(//|///|//!)' | wc -l

# Full spawn-edge sweep (Sweep B), for regression:
rg --no-heading -n 'Command::new\(' crates/devflow-core/src crates/devflow-cli/src
```

---

*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Census taken: 2026-07-30, against worktree HEAD `f539012f8656d37e41627c6015cf9bc4db509051` (waves 1–2 merged)*
