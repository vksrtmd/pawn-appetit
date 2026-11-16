#!/bin/sh
# A `test:` commit changes tests only. A change to app code under src/ makes it
# a `fix:` or `feat:`, so the behaviour change shows in the history.
#   check-test-commit.sh <commit-msg-file>   commit-msg hook: the staged change
#   check-test-commit.sh --range <a>..<b>    CI: every commit in the range
is_test() { printf '%s\n' "$1" | head -n1 | grep -Eq '^test(\([^)]*\))?!?:'; }
app_files() { grep -Ev '(/__tests__/|\.test\.[cm]?[jt]sx?$)'; }
fail() {
    echo "$1: a test: commit changes app code; commit it as fix: or feat:" >&2
    printf '%s\n' "$2" | sed 's/^/  /' >&2
    status=1
}
status=0
if [ "$1" = "--range" ]; then
    for c in $(git rev-list --no-merges "$2"); do
        is_test "$(git log -1 --format=%s "$c")" || continue
        app=$(git diff-tree --no-commit-id --name-only -r "$c" -- src | app_files)
        [ -n "$app" ] && fail "$(git log -1 --format='%h %s' "$c")" "$app"
    done
else
    if is_test "$(cat "$1")"; then
        app=$(git diff --cached --name-only -- src | app_files)
        [ -n "$app" ] && fail "staged commit" "$app"
    fi
fi
exit $status
