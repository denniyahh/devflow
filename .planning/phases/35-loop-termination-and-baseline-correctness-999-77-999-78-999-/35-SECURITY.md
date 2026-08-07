---
phase: 35
slug: loop-termination-and-baseline-correctness-999-77-999-78-999
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-08-07
---

# Phase 35 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
>
> Register built retroactively from all 6 PLAN.md `<threat_model>` blocks (State B — no
> pre-existing SECURITY.md). `register_authored_at_plan_time: true` for every plan; no SUMMARY.md
> flagged new threats during execution.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| DevFlow → the `git` child process | External binary whose availability/output DevFlow cannot assume; its failure modes cross into safety-gate arithmetic | commit counts, exit codes |
| Test process → process-global `PATH` | A test binary's threads share one environment; a guard held too long or dropped too late changes behaviour for unrelated tests | `PATH` env var |
| Persisted `State` → the stage dispatch arm | `worktree_path` selects which filesystem root the checkpoint declaration is read from | filesystem paths |
| Test fixture → the working tree | Task 2 of 35-02 deliberately makes committed production code wrong for the duration of one test run | source file contents |
| DevFlow → `ssh-keygen` child | External binary handling private key material; stdout/stderr/exit code cross back into operator-visible output | signing key material, exit codes |
| DevFlow → the shared system temporary directory | The signing probe creates a directory and writes a payload + signature there; the directory is world-reachable before creation | throwaway payload bytes |
| `release --check` → operator terminal | Every reason string is printed; the configured signing key value must never appear in it | reason strings |
| Persisted `.devflow/state-{NN}.json` → the running process | State survives across processes; the only thing carrying the bound between a crash/restart and the next run | `phase_validate_failures` counter |
| Unattended run → the human | The gate is the only mechanism converting an unbounded loop into a human decision | gate message text |
| A previous run's committed artifact → this run's dispatch decision | A file authored by an earlier run, carried on the feature branch, is read as though it were this run's verdict | `{N}-VERIFICATION.md` content |
| Persisted state → a later `devflow advance` process | The run-start baseline is written by one process and compared by another | content fingerprint |
| Published crate → any future consumer | The changelog and crate docs are the only signal a consumer gets since the operator declined a major version bump | public API surface |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-35-01 | Denial of Service | `phase_commit_count` → `handle_validate_outcome` baseline write | high | mitigate | `phase_commit_count` returns `Option<u32>`; baseline write confined to `Some` arm (`agent_result.rs:1881`, `pipeline_outcomes.rs:615-627`) | closed |
| T-35-02 | Spoofing | `evaluate_layer2` | medium | mitigate | Returns `Ok(None)` to Layer 3 rather than classifying a successful agent as `Failed` (`agent_result.rs:1969,1995`) | closed |
| T-35-02b | Spoofing | `evaluate_layer3` fall-through | high | mitigate | Layer 3 re-pointed at `phase_commit_count`; `None` classified `Unknown`, proven at cascade level (`agent_result.rs:2080-2124`) | closed |
| T-35-03 | Tampering | `NoGitPath` mutation of process-global `PATH` | medium | mitigate | RAII restore in `Drop` on every exit path incl. panic, single `PATH` mutex (`crates/devflow-cli/src/test_support.rs:389-421`) | closed |
| T-35-04 | Information Disclosure | new doc comments and test assertions | low | accept | See Accepted Risks Log | closed |
| T-35-SC | Tampering | package-manager installs (×6, one per plan) | low | accept | See Accepted Risks Log | closed |
| T-35-08 | Spoofing | `Action::GateReview` checkpoint auto-decide root selection | medium | mitigate | Test drives real dispatch arm, fails under reverted form (`pipeline_launch.rs:2421`) | closed |
| T-35-09 | Tampering | temporarily reverted working tree during Task 2 | medium | mitigate | Revert never committed; `git diff --exit-code` clean at HEAD | closed |
| T-35-10 | Denial of Service | checkpoint resume path in worktree mode | low | accept | See Accepted Risks Log | closed |
| T-35-11 | Information Disclosure | `NotViable`/`Unknown` reason strings | high | mitigate | Exit code is sole verdict; stderr `Stdio::null()`; fixed strings keyed by failure class, no path/key material (`git.rs:995-1003`) | closed |
| T-35-12 | Tampering | probe workspace under system temp dir | medium | mitigate | Non-recursive `DirBuilder::create` (mode 0o700), unique-per-call name via `AtomicU64` (`git.rs:868,926-946,958-963`) | closed |
| T-35-13 | Elevation of Privilege | `ssh-keygen` sign invocation | medium | mitigate | `ProbeWorkspace` Drop removes dir on every exit incl. unwind; signs only self-generated payload bytes (`git.rs:912-918`) | closed |
| T-35-14 | Denial of Service | `release --check` preflight | high | mitigate | `SSH_ASKPASS_REQUIRE=never` + wall-clock ceiling with kill/reap; `setsid` detachment regression-tested (`git.rs:995,1038-1064,3028`) | closed |
| T-35-15 | Spoofing | signing viability verdict | high | mitigate | Predictor replaced with real `ssh-keygen -Y sign` operation; old `classify_ssh_add_status`/`SigningStatus` absent from `crates/` | closed |
| T-35-16 | Denial of Service | the Code↔Validate loop | high | mitigate | Never-reset per-phase total (`state.rs:150`), ceiling above streak ceiling (`mode.rs:41,49`) | closed |
| T-35-17 | Repudiation | the Supervise gate message | medium | mitigate | Message leads with cumulative per-phase total, named as such (`pipeline_outcomes.rs:692`) | closed |
| T-35-18 | Elevation of Privilege | the ceiling gate's consequence | high | mitigate | Ceiling arm offers identical Advance/LoopBack/Abort choices as an ordinary gate — no abort-only path (`pipeline_outcomes.rs:711-724`) | closed |
| T-35-19 | Tampering | the carried-forward total | medium | mitigate | Carries exactly one field forward; absent/unreadable file means zero (`commands.rs:121,155`) | closed |
| T-35-20 | Denial of Service | saturating accumulation | low | mitigate | `saturating_add(1)` — cannot wrap to zero (`pipeline_outcomes.rs:589`) | closed |
| T-35-20b | Denial of Service | the ceiling reset in Supervise mode | high | mitigate | Reset keyed on `phase_failure_ceiling_reached` predicate, not "a gate fired" (`pipeline_outcomes.rs:648,718,724`) | closed |
| T-35-20c | Repudiation | `devflow start --dry-run` | medium | mitigate | Third probe branch names the per-phase ceiling in `print_dry_run` | closed |
| T-35-21 | Spoofing | `select_loop_back_fix` reading inherited verification artifact | high | mitigate | Content fingerprint compared against run-start baseline; unchanged-since-start → stale → full execute (`pipeline_outcomes.rs:316-390`) | closed |
| T-35-22 | Denial of Service | over-strict freshness rule | high | mitigate | Both directions tested; fresh case reaches gaps-only path (`pipeline_outcomes.rs:3674,3748`) | closed |
| T-35-22b | Denial of Service | NC-7 as sole guard on always-stale over-correction | high | mitigate | Decision extracted as pure predicate, all 4 truth-table rows pinned in committed test (`pipeline_outcomes.rs:3903`) | closed |
| T-35-23 | Tampering | the evidence-root probe | medium | mitigate | Prohibition against reverting to project root carried forward, asserted at capture site (`pipeline_outcomes.rs:299,529`) | closed |
| T-35-24 | Repudiation | fingerprint stability across processes | medium | mitigate | Explicit FNV-1a/64, not stdlib `DefaultHasher` (`agent_result.rs:2830-2851`) | closed |
| T-35-25 | Information Disclosure | the persisted fingerprint | low | accept | See Accepted Risks Log | closed |
| T-35-26 | Repudiation | the record of the public-API break | high | mitigate | Every changed/removed/added `pub` item enumerated and verified against source in CHANGELOG.md `## 2.5.0` | closed |
| T-35-26b | Repudiation | over-claimed breaking changes | medium | mitigate | `#[non_exhaustive]`/`#[serde(default)]` verified before classifying two new `State` fields additive (`CHANGELOG.md:120,297`) | closed |
| T-35-27 | Spoofing | the absence left by the removed predictor | medium | mitigate | 28-line removal note names both items, reason, replacement (`git.rs:724-748`) | closed |
| T-35-28 | Information Disclosure | changelog and doc text | low | accept | See Accepted Risks Log | closed |

*Status: open · closed · open — below {block_on} threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-35-01 | T-35-04 | Corrected doc comments and new test assertions carry no path, credential, or key material | plan author (35-01) | 2026-08-07 |
| AR-35-02 | T-35-10 | A wrong-root checkpoint read stalls a run rather than corrupting state; the phase's own gate machinery still fires | plan author (35-02) | 2026-08-07 |
| AR-35-03 | T-35-25 | The persisted fingerprint is a non-cryptographic hash of a planning document already committed to the repo; reveals nothing the artifact itself does not | plan author (35-05) | 2026-08-07 |
| AR-35-04 | T-35-28 | Changelog/doc text names symbol paths and backlog identifiers only; no operator path, host detail, or key material | plan author (35-06) | 2026-08-07 |
| AR-35-05 | T-35-SC (×6) | All six plans install zero new dependencies; independently confirmed via `git diff 749a151..HEAD -- **/Cargo.toml` (empty) | security auditor | 2026-08-07 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-08-07 | 34 | 34 | 0 | gsd-security-auditor |

**Auditor notes:** Verified against current implementation, not SUMMARY prose. Full workspace
`cargo check --workspace --all-targets` clean; `cargo test -p devflow-core --lib` → 576/0 failed;
`cargo test -p devflow --bin devflow` → 303/0 failed. Diff scan (`git diff 749a151..HEAD --stat`)
found one unaccounted change — a whitespace-only doc-comment rewrap in
`crates/devflow-cli/src/preflight.rs` (2 lines, no functional or security content); the
post-completion `setsid`/pty-detachment additions and the 35-REVIEW fixups (WR-03–WR-07, CR-01)
are deepenings of already-registered threats (T-35-14, T-35-17/T-35-21/T-35-23), not new attack
surface.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-08-07
