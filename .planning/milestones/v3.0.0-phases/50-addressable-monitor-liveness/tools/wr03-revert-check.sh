#!/usr/bin/env bash
# wr03-revert-check.sh - plan 50-26's revert check for WR-03.
#
# Purpose: prove that every new non-control WR-03 test discriminates. The
# script checks out HEAD in a scratch worktree, reverts there every commit
# whose subject starts with `fix(50-26): ` (newest first, `--no-commit`, so no
# commit is made and no hook runs), and runs each case with nextest in that
# tree, with its own CARGO_TARGET_DIR:
#   - fail-first A, C, F, D (devflow-core --lib) and E1, E3 (devflow --test
#     supervisor_e2e) must exit 100 with `WR-03:` on a panic's message line,
#     the line right after a `thread '...' panicked at ...` header;
#   - controls B and E2 must exit 0.
# The phase tree's `git status --porcelain` must be identical before and
# after: nothing in the phase tree may change while this runs.
#
# Usage:
#   wr03-revert-check.sh run       run the check (about 10-15 min: the scratch
#                                  tree builds from cold)
#   wr03-revert-check.sh recover   clean up after an interrupted run (a SIGKILL
#                                  skips the EXIT trap and leaves the scratch
#                                  worktree registered)
#
# Output: `REVERT ...` lines. Exit 0 only on `REVERT verdict=pass`. On any
# failure the per-case logs stay in target/wr03-revert-logs.
#
# What it does NOT establish:
#   - that the fixed code passes these tests (the plan's task gates run them);
#   - anything about a release binary: this is the debug test build, with the
#     devflow-core test-support feature, and a release binary has no
#     test-support hold, so it could not run E2 or E3;
#   - that the reverted tree fails for every possible regression: only that
#     these eight tests split as stated, on this host, in this run;
#   - anything about a live attempt.
#
# Kept in shell on purpose: it needs a second source tree, which no Rust test
# can build, and it decides on nextest's exit code plus the assertion marker.
# Only the recorded child pid is ever signalled, never a name or a pattern.
set -euo pipefail

W=$(git rev-parse --show-toplevel) || {
	echo "REVERT error=not_a_git_tree" >&2
	exit 1
}
S="$W/target/wr03-revert"
L="$W/target/wr03-revert-logs"
FIX_PREFIX="fix(50-26): "

CHILD_PID=""
VERDICT="fail"

# The cases, one per line: <kind> <name> <scope> <test>.
CASES="fail_first A core monitor::tests::a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout
control B core monitor::tests::a_completed_stream_run_returns_once_its_leftover_process_released_the_agents_stdout
fail_first C core monitor::tests::no_line_reaches_the_capture_after_the_stdout_wait_expires
fail_first F core monitor::tests::a_stream_run_returns_while_a_leftover_process_keeps_writing_to_the_agents_stdout
fail_first D core monitor::tests::a_stopped_stream_run_returns_while_a_process_outside_the_agent_group_holds_its_stdout
fail_first E1 cli a_completed_stream_run_reaches_its_advance_while_a_leftover_process_holds_stdout
control E2 cli a_completed_stream_run_reaches_its_advance_once_its_leftover_process_released_stdout
fail_first E3 cli a_stop_sent_while_a_completed_run_waits_on_a_held_stdout_ends_it_at_safe_point_1"

# Whether git registers the scratch worktree.
scratch_registered() {
	local listing
	listing=$(git -C "$W" worktree list --porcelain) || {
		echo "REVERT error=git_worktree_list" >&2
		exit 1
	}
	printf '%s\n' "$listing" | awk -v want="worktree $S" '$0 == want { found = 1 } END { exit !found }'
}

# Remove the scratch worktree: unregister it, remove its exact path, prune.
remove_scratch() {
	case "$S" in
	"$W"/target/wr03-revert) ;;
	*)
		echo "REVERT error=unexpected_scratch_path path=$S" >&2
		exit 1
		;;
	esac
	if scratch_registered; then
		git -C "$W" worktree remove --force "$S" >/dev/null 2>&1 ||
			echo "REVERT warning=worktree_remove_failed path=$S" >&2
	fi
	if [ -e "$S" ]; then
		rm -rf -- "$S"
	fi
	git -C "$W" worktree prune || echo "REVERT warning=worktree_prune_failed" >&2
}

# EXIT trap, idempotent: the scratch worktree always goes; the logs stay
# unless the verdict passed.
cleanup() {
	cd "$W" || return
	remove_scratch
	if [ "$VERDICT" = pass ] && [ -e "$L" ]; then
		rm -rf -- "$L"
	fi
}

# TERM/INT/HUP: TERM only the recorded child, wait for it, and stop.
on_signal() {
	if [ -n "$CHILD_PID" ]; then
		kill -TERM "$CHILD_PID" 2>/dev/null || true
		wait "$CHILD_PID" 2>/dev/null || true
	fi
	echo "REVERT interrupted"
	exit 143
}

# `yes` when a line that starts, after optional whitespace, with `thread '`
# and contains ` panicked at ` is immediately followed by a line containing
# `WR-03:` (the panic's own message line); `no` otherwise. Keyed on the
# previous line, never a search over the whole log.
marker_of() {
	if awk -v q="'" '
		prev_header && index($0, "WR-03:") > 0 { found = 1 }
		{
			line = $0
			sub(/^[[:space:]]+/, "", line)
			prev_header = (index(line, "thread " q) == 1 && index(line, " panicked at ") > 0)
		}
		END { exit !found }
	' "$1"; then
		echo yes
	else
		echo no
	fi
}

# Run one case in the scratch tree in the background, record its pid, and
# wait on it, so a trapped signal interrupts the wait at once. Sets CASE_RC.
run_case() {
	local name=$1 scope=$2 test=$3
	local -a package
	if [ "$scope" = core ]; then
		package=(-p devflow-core --lib)
	else
		package=(-p devflow --test supervisor_e2e)
	fi
	CASE_RC=0
	timeout --kill-after=30 900 cargo nextest run --color never "${package[@]}" \
		--no-tests=fail -E "test(=$test)" >"$L/$name.log" 2>&1 &
	CHILD_PID=$!
	wait "$CHILD_PID" || CASE_RC=$?
	CHILD_PID=""
}

do_run() {
	local path
	for path in "$S" "$L"; do
		if [ -e "$path" ]; then
			echo "REVERT scratch_exists path=$path"
			echo "an earlier run left $path: read target/wr03-revert-logs, then run '$0 recover'" >&2
			exit 1
		fi
	done
	if scratch_registered; then
		echo "REVERT scratch_exists path=$S"
		echo "git still registers $S: run '$0 recover'" >&2
		exit 1
	fi

	local before after log
	before=$(git -C "$W" status --porcelain) || {
		echo "REVERT error=git_status" >&2
		exit 1
	}

	log=$(git -C "$W" log --format='%H%x09%s' HEAD) || {
		echo "REVERT error=git_log" >&2
		exit 1
	}
	local -a fixes
	mapfile -t fixes < <(printf '%s\n' "$log" | awk -F '\t' -v prefix="$FIX_PREFIX" 'index($2, prefix) == 1 { print $1 }')
	echo "REVERT fix_commits=${#fixes[@]}"
	if [ "${#fixes[@]}" -eq 0 ]; then
		echo "REVERT verdict=fail"
		exit 1
	fi

	mkdir -p -- "$L"
	trap cleanup EXIT
	trap on_signal TERM INT HUP

	git -c core.hooksPath=/dev/null -C "$W" worktree add -q --detach "$S" HEAD || {
		echo "REVERT error=worktree_add" >&2
		exit 1
	}
	local sha
	for sha in "${fixes[@]}"; do
		if ! git -c core.hooksPath=/dev/null -C "$S" revert --no-commit "$sha" >"$L/revert-${sha:0:12}.log" 2>&1; then
			echo "REVERT revert_conflict commit=${sha:0:12}"
			echo "REVERT logs_kept=$L"
			echo "REVERT verdict=fail"
			exit 1
		fi
	done

	cd "$S"
	export CARGO_TARGET_DIR="$S/target"
	local bad=0 kind name scope test marker
	while read -r kind name scope test; do
		run_case "$name" "$scope" "$test"
		if [ "$kind" = fail_first ]; then
			marker=$(marker_of "$L/$name.log")
			echo "REVERT fail_first=$name rc=$CASE_RC marker=$marker"
			if [ "$CASE_RC" -ne 100 ] || [ "$marker" != yes ]; then
				bad=1
			fi
		else
			echo "REVERT control=$name rc=$CASE_RC"
			if [ "$CASE_RC" -ne 0 ]; then
				bad=1
			fi
		fi
	done <<<"$CASES"

	cd "$W"
	unset CARGO_TARGET_DIR
	remove_scratch
	after=$(git -C "$W" status --porcelain) || {
		echo "REVERT error=git_status" >&2
		exit 1
	}
	if [ "$before" = "$after" ]; then
		echo "REVERT phase_tree_unchanged=yes"
	else
		echo "REVERT phase_tree_unchanged=no"
		bad=1
	fi

	if [ "$bad" -eq 0 ]; then
		VERDICT=pass
		rm -rf -- "$L"
		echo "REVERT verdict=pass"
		exit 0
	fi
	echo "REVERT logs_kept=$L"
	echo "REVERT verdict=fail"
	exit 1
}

do_recover() {
	remove_scratch
	if [ -e "$L" ]; then
		echo "REVERT logs at $L:"
		ls -1 -- "$L"
		rm -rf -- "$L"
	fi
	echo "REVERT recovered"
}

case "${1:-}" in
run) do_run ;;
recover) do_recover ;;
*)
	echo "usage: $0 run|recover" >&2
	exit 2
	;;
esac
