#!/usr/bin/env bash
# Fail when a workspace branch's commit differs from develop in a way it has
# not declared in .workspace-divergence.
#
# Usage: scripts/check-workspace-divergence.sh [BASE_REF] [SHA]
#        (defaults: origin/develop, HEAD)
#
# The rule (CONTRIBUTING.md, Workflow B): the workspace adds files and does not
# edit develop's files, except the ones it declares.
#   - A path that exists only here must be declared `only <path>`, or be one of
#     cut-pr-branch.sh's built-in never-upstream paths.
#   - A path that exists on develop and differs here (modified or deleted) must
#     be declared `modified <path>`, or be a built-in never-upstream path.
#   - A declaration must still be true: an `only` path that develop now carries,
#     or a `modified` path that no longer differs, is stale and fails too.
#   - Every hook and scripts/*.sh must be committed executable.
# Every input is read from SHA, never from the checkout or its index: the
# manifest, the diff against BASE_REF, the committed modes, and the built-in
# list, which comes from scripts/cut-pr-branch.sh at SHA so the two cannot
# drift. scripts/hooks/pre-push runs this once for each pushed commit that
# carries a manifest; run by hand it checks HEAD. It compares against the local
# BASE_REF, so it needs no network; `git fetch` first for an up-to-date
# comparison.
set -euo pipefail

BASE_REF="${1:-origin/develop}"
SHA="${2:-HEAD}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

if ! git rev-parse --verify --quiet "$BASE_REF^{commit}" >/dev/null; then
    echo "check-workspace-divergence: base ref '$BASE_REF' not found." >&2
    exit 1
fi
if ! git rev-parse --verify --quiet "$SHA^{commit}" >/dev/null; then
    echo "check-workspace-divergence: commit '$SHA' not found." >&2
    exit 1
fi

if ! CUT_SCRIPT="$(git show "$SHA:scripts/cut-pr-branch.sh" 2>/dev/null)"; then
    echo "check-workspace-divergence: $SHA carries no scripts/cut-pr-branch.sh to read FORBIDDEN_PATHS from." >&2
    exit 1
fi
BUILTIN_LINE="$(sed -n 's/^FORBIDDEN_PATHS=(\(.*\))$/\1/p' <<< "$CUT_SCRIPT")"
if [ -z "$BUILTIN_LINE" ]; then
    echo "check-workspace-divergence: could not read FORBIDDEN_PATHS from scripts/cut-pr-branch.sh at $SHA." >&2
    exit 1
fi
read -r -a BUILTIN <<< "$BUILTIN_LINE"

# A commit with no manifest declares nothing.
ONLY=()
MODIFIED=()
if git cat-file -e "$SHA:.workspace-divergence" 2>/dev/null; then
    if ! MANIFEST="$(git show "$SHA:.workspace-divergence")"; then
        echo "check-workspace-divergence: could not read .workspace-divergence at $SHA." >&2
        exit 1
    fi
    while read -r KIND ENTRY _; do
        case "$KIND" in
            ''|'#'*) ;;
            only) ONLY+=("${ENTRY%/}") ;;
            modified) MODIFIED+=("$ENTRY") ;;
            *)
                echo "check-workspace-divergence: unknown line kind '$KIND' in .workspace-divergence." >&2
                exit 1
                ;;
        esac
    done <<< "$MANIFEST"
fi

# Exact path or anything under it, never a mere name prefix.
under() { # under <path> <entry>...
    local path="$1" entry
    shift
    for entry in "$@"; do
        entry="${entry%/}"
        if [ "$path" = "$entry" ] || [ "${path#"$entry"/}" != "$path" ]; then
            return 0
        fi
    done
    return 1
}

# Each listing is captured before it is read: a `done < <(git ...)` loop does
# not see git fail, reads nothing and passes.
if ! CHANGED="$(git diff --name-status --no-renames "$BASE_REF" "$SHA")"; then
    echo "check-workspace-divergence: could not diff $BASE_REF against $SHA." >&2
    exit 1
fi

problems=()
while IFS=$'\t' read -r STATUS PATH_A PATH_B; do
    [ -n "$STATUS" ] || continue
    path="${PATH_B:-$PATH_A}"
    under "$path" "${BUILTIN[@]}" && continue
    case "$STATUS" in
        A*)
            under "$path" "${ONLY[@]+"${ONLY[@]}"}" || problems+=("undeclared workspace-only path: $path (declare: only $path)")
            ;;
        *)
            printf '%s\n' "${MODIFIED[@]+"${MODIFIED[@]}"}" | grep -Fxq -- "$path" \
                || problems+=("undeclared change to a develop file: $path (move it to develop, or declare: modified $path)")
            ;;
    esac
done <<< "$CHANGED"

for entry in "${ONLY[@]+"${ONLY[@]}"}"; do
    if git cat-file -e "$BASE_REF:$entry" 2>/dev/null; then
        problems+=("stale declaration: 'only $entry' but $BASE_REF carries it")
    fi
done
for entry in "${MODIFIED[@]+"${MODIFIED[@]}"}"; do
    if git diff --quiet "$BASE_REF" "$SHA" -- "$entry"; then
        problems+=("stale declaration: 'modified $entry' but it matches $BASE_REF")
    fi
done

# Git runs only executable hooks, and core.fileMode=false checkouts record new
# files as 100644: a hook or script committed that way silently never runs on a
# fresh clone. Check the mode the commit carries, not the index's or the
# working tree's. `git ls-tree` refuses glob pathspec magic, so the listing is
# the whole of scripts/ and is narrowed here: everything under scripts/hooks/,
# and scripts/<name>.sh at the top level only (scripts/lib/ is sourced).
if ! SCRIPT_MODES="$(git ls-tree -r "$SHA" -- scripts)"; then
    echo "check-workspace-divergence: could not list the scripts $SHA carries." >&2
    exit 1
fi
while IFS=$'\t' read -r META FILE; do
    case "$FILE" in
        scripts/hooks/*) ;;
        scripts/*/*) continue ;;
        scripts/*.sh) ;;
        *) continue ;;
    esac
    MODE="${META%% *}"
    if [ "$MODE" != "100755" ]; then
        problems+=("not committed executable ($MODE): $FILE (git update-index --chmod=+x $FILE)")
    fi
done <<< "$SCRIPT_MODES"

if [ "${#problems[@]}" -gt 0 ]; then
    echo "check-workspace-divergence: $SHA diverges from $BASE_REF in ways .workspace-divergence does not declare:" >&2
    printf '  %s\n' "${problems[@]}" >&2
    exit 1
fi
echo "check-workspace-divergence: every difference from $BASE_REF is declared."
