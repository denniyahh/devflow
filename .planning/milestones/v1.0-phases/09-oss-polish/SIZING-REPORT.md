# Phase 9 OSS Polish — Sizing & Prioritization Report

**Date:** 2026-06-18 | **Source:** `PLAN.md` (Codex-refined) + `CONTEXT.md` | **Mode:** read-only analysis

---

## 1. Item-by-Item Sizing

Effort key: **S** < 30min · **M** 30–90min · **L** 90min–3hr · **XL** > 3hr.

| Section | Title | Effort | Risk | Value | Priority | Dependencies |
|---|---|---|---|---|---|---|
| 9.0 | Preflight & scope fence | S | Low | Med | **P0** | — (must run first) |
| 9.1 | Remove OMX agent support | M | Med | High | **P0** | 9.0 |
| 9.2 | Remove Dennis-specific assumptions | M | Med | High | **P0** | 9.0; pairs with 9.4 (`.planning/` doc) |
| 9.3 | Ship branch safety fix | M | **High** | High | **P0** | 9.0 |
| 9.4 | Architecture docs + agent verification | L | Low | High | **P0** | 9.1, 9.3 (must reflect post-fix state) |
| 9.5 | Public docs correctness | L | Low | High | **P0** | 9.1, 9.3, 9.4 |
| 9.6 | Dev container + distrobox | M | Med | Med | **P1** | 9.2 |
| 9.7 | CI & toolchain polish | S–M | Low | Med | **P1** | — (independent) |

### Sizing rationale (the non-obvious calls)

- **9.1 — Med, not Small.** "Delete a file" undersells it. OMX is woven through `omx.rs`, `agents/mod.rs` (module + export + `adapter_for`), `state.rs` (enum/parser/display/disabled comments), tests, `.omx/` runtime dir, and 4 references in the Hermes skill plus README/CONTRIBUTING/DEPENDENCIES. Risk is Med because edits to the `AgentKind` enum and parser must keep tests green.
- **9.2 — Med / Med risk.** The `.planning/` decision is a genuine judgment call, not a mechanical edit: `phase_prompt()` hardcodes `.planning/ROADMAP.md` and `.planning/phases/{phase}/CONTEXT.md`, so the docs must commit to "`.planning/` is the convention." Narrowing `.gitignore` risks accidentally untracking docs — verify with `git ls-files` after.
- **9.3 — Med effort but the highest-risk item.** This is the only change to live git orchestration logic (`release_start()` checkout semantics). A wrong fix can corrupt the release→main/develop merge flow or the `v{version}` tag path. The regression test (release tip descends from feature HEAD) is mandatory, and 9.3 must verify `release_finish()` is unaffected.
- **9.4 — Large.** Nine ARCHITECTURE.md sections, each requiring source-of-truth verification against `state.rs`, `workflow.rs`, `git.rs`, `config.rs`, `agent_result.rs`, `monitor.rs`, `worktree.rs`. This is the single biggest line-count contributor.
- **9.5 — Large.** Touches README, CONTRIBUTING, CHANGELOG, DEPENDENCIES, and the Hermes skill, with multiple correctness fixes each (command table, `.devflow.yaml` schema, completion-eval layers, rate-limit claims, fork PR workflow). High coordination cost; low technical risk.
- **9.6 — Med / Med risk.** The `devcontainer.json` is pre-specified (low authoring cost), but **verification is the risk**: the immutable Fedora Kinoite host cannot easily build/run the container locally, so "container works" must fall back to documented manual `cargo` checks rather than a live test.
- **9.7 — S–M.** Badge + CI audit are quick; the only real decision is the toolchain policy (`rust-toolchain.toml` with `stable` vs. removing the unverified `1.91+` claims). Recommend the stable-channel file — it's cheap and ends the ambiguity.

---

## 2. Priority-Ordered Execution Sequence

Dependency-correct order (matches the plan's intent, with priorities overlaid):

1. **9.0** Preflight & scope fence — *gate, cheap*
2. **9.1** Remove OMX — *unblocks accurate agent docs*
3. **9.3** Ship branch safety fix — *highest risk; do it early while context is fresh, before docs describe its behavior*
4. **9.2** Remove Dennis-specific assumptions — *`.planning/` decision feeds 9.4*
5. **9.4** Architecture docs — *depends on 9.1 + 9.3 being final*
6. **9.5** Public docs correctness — *consumes 9.4's decisions; closes CHANGELOG claims*
7. **9.7** CI & toolchain polish — *independent; can slot anywhere after 9.0*
8. **9.6** Dev container + distrobox — *last; lowest value, hardest to verify on this host*

> Note: this reorders 9.2 and 9.3 relative to the plan's numbering (plan runs 9.2→9.3). Doing **9.3 before 9.2** keeps the riskiest code change isolated and ensures 9.4 documents final ship semantics. The plan's 9.2→9.3 order is also fine; the only hard constraint is **9.1 + 9.3 before 9.4 before 9.5**.

---

## 3. Recommended Capacity & Fit

**Historical baseline:** DevFlow phases average 4–6 commits, ~300–500 lines changed. The plan proposes **6 commits**.

**Verdict: the 6-commit count fits; the line-count budget does not.**

- Commit count (6) sits exactly at the top of the historical band — acceptable.
- **Effort sum is the problem.** Two **Large** doc sections (9.4 + 9.5) plus four M/S code sections total well over 3 hours and, more importantly, ARCHITECTURE.md + a full README/CONTRIBUTING/CHANGELOG rewrite will **blow past the 300–500 line envelope** on their own. This is a doc-heavy phase masquerading as a 6-commit code phase.

**Recommended capacity for one clean phase:** the **P0 spine** — 9.0, 9.1, 9.2, 9.3, plus the *core* of 9.4/9.5. That is the functional-correctness work (supported-agents-only, ship bug, accurate architecture) and lands inside the historical commit band.

### What fits cleanly (keep in Phase 9)
- 9.0, 9.1, 9.2, 9.3 — all P0, all S/M, the functional backbone.
- 9.4, 9.5 — P0 for OSS credibility, but **flag for line-budget overrun** (see §4 option A).

### What is at the margin
- 9.6, 9.7 — both P1. Genuinely useful but neither blocks a launch, and 9.6 can't be verified well on this host.

---

## 4. Overflow Items & Suggested Destination

None of the eight sections are audit-log or agent-registry work, so **nothing maps to Phase 11** (audit log / release workflow — already correctly fenced out by the plan). The overflow here is **line-budget pressure**, not misplaced scope. Two ways to handle it:

**Option A — Split the phase (recommended if holding the 300–500 line band is firm):**
- **Phase 9 (functional):** commits 1–3 → 9.0, 9.1, 9.2, 9.3. Pure correctness; small, reviewable, low-doc.
- **Phase 9.5 / follow-up (docs):** commits 4–6 → 9.4, 9.5, 9.6, 9.7. The big doc + infra payload, reviewed as a documentation pass.

This keeps each slice inside the historical envelope and isolates the risky git change from the noisy doc diff.

**Option B — Trim P1 to a follow-up (if keeping a single 6-commit phase):**
- Keep 9.0–9.5 in Phase 9 (all P0).
- **Defer 9.6 (dev container + distrobox-optional) → Phase 10 (Antigravity + Hermes agent support).** Rationale: 9.6 is contributor-*environment* work, and Phase 10 already reopens the contributor/agent-onboarding surface (Hermes skill, new adapters). The devcontainer naturally rides along with that onboarding pass, and 9.6 is the section least verifiable on the current immutable host — deferring it removes the weakest verification story from the launch phase. `distrobox.ini` removal is trivial and can stay in 9.2 regardless.
- Keep 9.7 (P1 but S–M and independent) — it's cheap and CI visibility is part of a credible launch.

**Recommendation:** Option B if the team wants a single shippable OSS-launch phase; Option A if review hygiene / line-budget discipline is the priority. Do **not** push 9.4 or 9.5 out — inaccurate architecture/README docs undercut the entire "OSS-ready" goal and are P0.

---

## 5. Risk Mitigation Notes (uncertain estimates)

| Item | Uncertainty | Mitigation |
|---|---|---|
| **9.3 Ship fix** | **Highest.** Effort could jump M→L if `release_start()`'s checkout coupling has hidden callers in the CLI `ship`/`confirm` path, or if `release_finish()` implicitly relied on starting from `develop`. | Write the failing regression test **first** (feature branch with a unique commit → assert release tip descends from it). Confirm `release_finish()` still merges to main+develop and tags `v{version}` *before* committing. This is the one item that can break a working `devflow ship`. |
| **9.2 `.planning/` decision** | Med. The "document as convention vs. make configurable" choice changes scope: documenting is S, making `phase_prompt()` configurable is L+. Plan explicitly chooses *document the current convention* — hold that line; making it configurable is scope creep. | Lock the decision at 9.0 (scope fence). Verify `git ls-files` after any `.gitignore` narrowing so docs aren't silently untracked. |
| **9.4 Architecture doc** | Med. Nine sections × source verification; easy to under-budget. Drift risk if written before 9.1/9.3 land. | Sequence strictly after 9.1 + 9.3. Treat each section's "Source of Truth" column as a checklist — verify against the named file, don't write from memory. |
| **9.6 Container verify** | Med. Cannot be validated on the immutable Fedora Kinoite host; "works in <2 min" is unverifiable locally. | Accept JSON-validity + documented manual `cargo build/test/fmt/clippy` fallback as the verification bar. Do not claim a live container test that wasn't run. Reinforces the Option B case for deferring to Phase 10. |
| **9.1 OMX grep** | Low–Med. The "zero references" gate may catch historical `.planning/` artifacts or CHANGELOG entries the team wants to keep. | The plan already scopes the grep to `crates` + named public docs and allows intentional changelog/history retention outside that set. Keep the grep path-scoped; don't chase historical planning files. |
| **Line-budget** | Med. 9.4 + 9.5 combined are the most likely single cause of exceeding 300–500 lines. | Decide §4 Option A vs. B **before** starting commit 4, not after the diff balloons. |

---

## TL;DR

- **P0 (must-have for launch):** 9.0, 9.1, 9.2, 9.3, 9.4, 9.5.
- **P1 (should-have):** 9.6, 9.7.
- **Commit count (6) fits; line-budget is the real constraint** — driven by the two Large doc sections.
- **Highest-risk item is 9.3** (live git/ship logic) — test-first, verify `release_finish()` unaffected.
- **No items belong in Phase 11** (audit/release already fenced out). If trimming, **defer 9.6 → Phase 10** (contributor-environment work pairs with agent onboarding); keep all docs (9.4/9.5) in-phase.
- Recommended path: **Option B** for a single launch phase, or **Option A** (functional vs. docs split) if line-budget discipline is firm.
