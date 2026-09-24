#!/usr/bin/env bash
# Refuse source commits made outside the active phase's worktree.
#
# The failure this exists to stop: a phase's worktree is created, and then work
# lands on the main checkout's branch anyway — because `cd` does not persist
# across an agent's separate tool calls, because an earlier `git checkout`
# silently changed what a much-later `git commit` targets (CLAUDE.md records
# both), or simply because nothing ever checked. The commit succeeds, so
# nothing surfaces until someone tries to separate the phase's work again.
#
# Scope is deliberately asymmetric, and the asymmetry is the point:
#
#   crates/**                    -> HARD REFUSE. Once the worktree exists there
#                                   is no legitimate reason for phase source to
#                                   land elsewhere, and source is the expensive
#                                   thing to move afterwards.
#   .planning/phases/<N>-*/**    -> WARN ONLY. Early spec commits made before
#                                   the worktree was populated are legitimate
#                                   and routine; docs are cheap to move. Warning
#                                   mirrors post-commit's DEV-SETUP-CHECKLIST
#                                   nudge rather than inventing a new severity.
#
# The escape hatch is DEVFLOW_ALLOW_OFF_WORKTREE=1, NOT `--no-verify`. Agents
# reach for `--no-verify` reflexively and it disables every other guard in this
# directory at the same time; a named variable is deliberate and greppable in
# shell history.
#
# Exercised end-to-end by scripts/test-phase-worktree-guard.sh, which asserts
# BOTH directions -- a guard only ever observed passing has not been shown to
# discriminate.
set -euo pipefail

# `--staged` is the only supported mode today; the argument is required so a
# future non-staged mode cannot be introduced by accident.
if [ "${1:-}" != "--staged" ]; then
    echo "usage: $0 --staged" >&2
    exit 2
fi

if [ "${DEVFLOW_ALLOW_OFF_WORKTREE:-0}" = "1" ]; then
    exit 0
fi

toplevel="$(git rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$toplevel" ] || exit 0

state_file="$toplevel/.planning/STATE.md"
[ -f "$state_file" ] || exit 0

# `current_phase:` in STATE.md's YAML frontmatter. Decimal phases (35.3) are
# real here, so the pattern is not [0-9]+. Read only the frontmatter's first
# occurrence: prose further down the document also says "current_phase".
phase="$(awk '
    /^current_phase:[[:space:]]*/ {
        sub(/^current_phase:[[:space:]]*/, "")
        gsub(/[[:space:]"]/, "")
        print
        exit
    }' "$state_file" 2>/dev/null || true)"

case "$phase" in
    ''|*[!0-9.]*) exit 0 ;;   # absent, or not a phase number -> nothing to enforce
esac

phase_branch="feature/phase-${phase}"

# Ask git which worktrees exist rather than probing `.worktrees/phase-N` on
# disk: the directory convention is a convention, the checkout is the fact.
# This also keeps the answer correct when the guard runs from INSIDE a
# worktree, where the relative `.worktrees/` path does not resolve at all.
worktree_exists=0
while read -r line; do
    case "$line" in
        "branch refs/heads/${phase_branch}") worktree_exists=1; break ;;
    esac
done < <(git worktree list --porcelain 2>/dev/null || true)

[ "$worktree_exists" -eq 1 ] || exit 0

current_branch="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
[ -n "$current_branch" ] || exit 0                 # detached HEAD: not our call
[ "$current_branch" != "$phase_branch" ] || exit 0 # already in the right place

# NUL-separated, because the newline form quotes any name holding a non-ASCII
# byte (under the default core.quotePath) or a quote, backslash or control
# character, and a quoted `"crates/...` line never matched `^crates/`.
# --no-relative pins repository-root names: diff.relative=true would otherwise
# drop every staged path outside the directory the guard was started from.
staged_source=""
staged_phase_docs=""
while IFS= read -r -d '' path; do
    case "$path" in
        crates/*) staged_source+="${path}"$'\n' ;;
        ".planning/phases/${phase}-"*) staged_phase_docs+="${path}"$'\n' ;;
    esac
done < <(git diff --cached --name-only -z --no-relative || true)

if [ -n "$staged_source" ]; then
    worktree_path="$(git worktree list --porcelain 2>/dev/null \
        | awk -v b="branch refs/heads/${phase_branch}" '
            /^worktree /{ p = substr($0, 10) }
            $0 == b     { print p; exit }' || true)"
    echo "pre-commit: refusing to commit phase ${phase} source on '${current_branch}'." >&2
    echo "  Phase ${phase} has a worktree and this is not it:" >&2
    printf '%s' "$staged_source" | sed 's/^/      /' >&2
    echo "  Commit source for this phase on '${phase_branch}' instead:" >&2
    if [ -n "$worktree_path" ]; then
        echo "      git -C ${worktree_path} ..." >&2
    fi
    echo "  Your staged changes are untouched. To carry them over:" >&2
    echo "      git stash push --staged && git -C <worktree> stash pop" >&2
    echo "  If this really is intentional (rare), set DEVFLOW_ALLOW_OFF_WORKTREE=1." >&2
    echo "  Do NOT use --no-verify: it disables every other guard in this hook too." >&2
    exit 1
fi

if [ -n "$staged_phase_docs" ]; then
    echo "pre-commit: WARNING — phase ${phase} docs committed on '${current_branch}'," >&2
    echo "  not on '${phase_branch}'. Legitimate before the worktree is populated;" >&2
    echo "  worth a second look afterwards. Not blocking." >&2
fi

exit 0
