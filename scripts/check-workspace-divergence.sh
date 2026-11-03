#!/usr/bin/env bash
# Fail when this workspace branch differs from develop in a way it has not
# declared in .workspace-divergence.
#
# Usage: scripts/check-workspace-divergence.sh [BASE_REF]   (default origin/develop)
#
# The rule (CONTRIBUTING.md, Workflow B): the workspace adds files and does not
# edit develop's files, except the ones it declares.
#   - A path that exists only here must be declared `only <path>`, or be one of
#     cut-pr-branch.sh's built-in never-upstream paths.
#   - A path that exists on develop and differs here (modified or deleted) must
#     be declared `modified <path>`, or be a built-in never-upstream path.
#   - A declaration must still be true: an `only` path that develop now carries,
#     or a `modified` path that no longer differs, is stale and fails too.
#   - Every tracked hook and scripts/*.sh must be committed executable.
# The built-in list is read from scripts/cut-pr-branch.sh so the two cannot
# drift. Runs from scripts/hooks/pre-push.d/ against the local BASE_REF, so it
# needs no network; `git fetch` first for an up-to-date comparison.
set -euo pipefail

BASE_REF="${1:-origin/develop}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

if ! git rev-parse --verify --quiet "$BASE_REF^{commit}" >/dev/null; then
    echo "check-workspace-divergence: base ref '$BASE_REF' not found." >&2
    exit 1
fi

BUILTIN_LINE="$(sed -n 's/^FORBIDDEN_PATHS=(\(.*\))$/\1/p' scripts/cut-pr-branch.sh)"
if [ -z "$BUILTIN_LINE" ]; then
    echo "check-workspace-divergence: could not read FORBIDDEN_PATHS from scripts/cut-pr-branch.sh." >&2
    exit 1
fi
read -r -a BUILTIN <<< "$BUILTIN_LINE"

ONLY=()
MODIFIED=()
if [ -f .workspace-divergence ]; then
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
    done < .workspace-divergence
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

problems=()
while IFS=$'\t' read -r STATUS PATH_A PATH_B; do
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
done < <(git diff --name-status --no-renames "$BASE_REF" HEAD)

for entry in "${ONLY[@]+"${ONLY[@]}"}"; do
    if git cat-file -e "$BASE_REF:$entry" 2>/dev/null; then
        problems+=("stale declaration: 'only $entry' but $BASE_REF carries it")
    fi
done
for entry in "${MODIFIED[@]+"${MODIFIED[@]}"}"; do
    if git diff --quiet "$BASE_REF" HEAD -- "$entry"; then
        problems+=("stale declaration: 'modified $entry' but it matches $BASE_REF")
    fi
done

# Git runs only executable hooks, and core.fileMode=false checkouts record new
# files as 100644: a hook or script committed that way silently never runs on a
# fresh clone. Check the committed mode, not the working tree's.
while read -r MODE _ _ FILE; do
    if [ "$MODE" != "100755" ]; then
        problems+=("not committed executable ($MODE): $FILE (git update-index --chmod=+x $FILE)")
    fi
done < <(git ls-files -s -- ':(glob)scripts/hooks/**' ':(glob)scripts/*.sh')

if [ "${#problems[@]}" -gt 0 ]; then
    echo "check-workspace-divergence: this branch diverges from $BASE_REF in ways .workspace-divergence does not declare:" >&2
    printf '  %s\n' "${problems[@]}" >&2
    exit 1
fi
echo "check-workspace-divergence: every difference from $BASE_REF is declared."
