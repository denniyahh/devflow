# HANDOFF — split 999.5 out of `feature/phase-26` and merge it to `develop`

**Written:** 2026-07-30, at the end of a long Phase 26 dogfood session.
**Status:** analysis and verification done; the merge itself is NOT done.
**Pick up with:** "read `.planning/HANDOFF-999.5-split.md` and continue".

---

## 1. What you asked for, and where it stopped

You asked to push Phase 26's work to `develop` (not `main`). On inspection the
branch turned out to mix genuinely-sound work with code carrying **5 open
Critical review findings**, so you chose to **surgically split out only the
sound part (999.5, the CHANGELOG content generation)** and merge that, leaving
the defective release-executor code on the branch.

That split was analysed and *proven to work*, then deliberately reverted so it
could be done properly in a fresh session. **Nothing has been merged.**

---

## 2. Verified state (checked 2026-07-30, trust but re-verify)

| | |
|---|---|
| `develop` | `72a1cea` — clean, nothing pushed |
| `feature/phase-26` | `f1b885d` — **74 commits ahead** of develop, unmerged |
| Worktrees | main checkout + `.worktrees/phase-26` only; no strays |
| Merged to develop | **nothing** — no phase-26 code, no tag, no version bump |
| `Cargo.toml` | still `2.1.0` in both places |

---

## 3. THE TASK — the exact operation to perform

Cherry-pick **three commits** from `feature/phase-26` onto `develop`. These are
the entirety of plan 26-02 (backlog item **999.5**, CHANGELOG content
generation):

```
a04b1bf  feat(26-02): generate CHANGELOG body from conventional-commit classification
b7252c3  feat(26-02): lock the complete conventional-type to heading mapping (D-12)
b62d3e6  feat(26-02): sanitize commit-derived changelog text (ASVS V7)
```

They touch exactly four files:
- `crates/devflow-core/src/version.rs`
- `crates/devflow-core/src/ship.rs`
- `crates/devflow-core/src/hooks.rs`
- `crates/devflow-cli/src/pipeline_outcomes.rs`

**This was already tested and applies cleanly** — all three cherry-picked with
zero conflicts (264+166+86 insertions), then the test branch was deleted. You
should still re-run it rather than trusting this note.

### Why it's clean despite sharing `version.rs` with the executor

`version.rs` is touched by both 999.5 *and* the executor commit `abc7f4a`
(`test(26-06): add failing test for release executor version bump`), which
extracts a new `version_in_contents()` helper out of `read_version()` so the
executor can read a version from `origin/main`'s *text*. But:

- `abc7f4a` comes **after** all three 999.5 commits in history, and
- it touches **different functions** — none of 999.5's `ChangelogHeading` /
  `changelog_sections` / `render_changelog_body` / `sanitize_changelog_subject`.

So replaying just the 999.5 commits reproduces the file as it was *before* the
executor refactor existed. No conflict. **The executor's `version_in_contents`
refactor is deliberately left behind** — it belongs to 999.25, not 999.5.

### After cherry-picking

1. `cargo test --workspace` — **assert on real `test result:` pass counts, not
   the exit code.** A `| tail` pipeline masks cargo's status and has produced a
   false green in this project more than once.
2. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`.
3. `develop` is a **protected branch** (`enforcement: active`, empty bypass
   list) — so this must go through a **PR**, not a direct push.

---

## 4. Gotchas that already bit this session — do not repeat

- **`git worktree add` failing silently.** A chained
  `git worktree add … && cd … && git checkout -b … && cherry-pick …` had the
  worktree-add fail (`develop` was already checked out elsewhere), the `cd`
  then failed, and the branch-creation + cherry-picks **ran in the main
  checkout instead**. Caught and reverted with no loss, but: **verify each step
  landed where you think before running the next one.** Do not chain
  worktree/cd/mutate.
- **`--include` is a `grep` flag, not ripgrep's.** Use `rg -g '*.rs'`. The wrong
  flag silently returns nothing and reads as a clean result.
- **`cargo test -p devflow-core` alone under-reports.** It fails to enable the
  `test-support` feature that `devflow-cli`'s dev-dependency turns on via
  feature unification, breaking 3 integration targets. Use `--workspace`.

---

## 5. Context: why Phase 26 ended this way

Phase 26 was dogfooded end-to-end through DevFlow itself. It reached **11/11
verification** with **763 tests passing**, then two independent code reviews
found Critical defects in the release executor: **7 Criticals**, then after a
fix round, **5 more** (fix scorecard: 1 closed / 5 partially-closed / 1
regressed). **Every Critical in both rounds was invisible to the test suite.**

Because a second automated fix round converged on *instances* rather than
*classes*, the executor (**999.25**) was re-opened and deferred rather than
fix-looped a third time.

Recorded in `.planning/ROADMAP.md`:
- **Phase 26** — `STATUS: CLOSED PARTIAL`
- **999.25** (release executor, DEN-50) — **re-opened**, with the 5 Criticals,
  the salvageable pieces, and 5 prerequisites
- **999.39** (`GIT_DIR` scrubbing, DEN-66) — **escalated Medium → High**; now
  blocks DEN-50 in Linear
- Full review: `.planning/phases/26-release-cut-automation/26-REVIEW.md`

**Caveat on "CLOSED PARTIAL":** that status is a *documentation* state. It
records an intent; it did not move, quarantine, or merge any code. The
defective 999.25 code still sits in `feature/phase-26` alongside 999.5. This
handoff's task is the first step that actually acts on that.

---

## 6. Open items after this merge lands

1. **Phase 27 = 999.39** (`GIT_DIR` scrubbing, DEN-66). Agreed it should be its
   own phase, not folded into 26. **Not yet scoped** — no ROADMAP Phase 27
   entry exists.
2. **999.25 re-attempt** — gated on 999.39 landing first.
3. **W-17 (operator action, deliberately deferred).** The `develop` ruleset
   `develop-merge-or-squash` is `enforcement: active` with an **empty bypass
   list** (confirmed live via `gh api`). DevFlow's direct-push design cannot
   land until you add a bypass. **Leave it as-is for now** — that enforcement is
   currently the only thing stopping the known-defective executor from reaching
   `origin`. Do it immediately before 999.25's UAT, never before.
4. `feature/phase-26` should be **kept**, not deleted — it is 999.25's starting
   point.
5. Lower-severity findings still open: WR-02, WR-04 (escalated), WR-05, WR-06,
   WR-07, IN-01 (escalated to contributing cause of CR-05).
