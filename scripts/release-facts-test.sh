#!/usr/bin/env bash
# shellcheck disable=SC2015 # echo cannot fail: A && ok || fail is if-then-else here
# release.sh's data-migration facts, in a throwaway git repo (no network):
# `just release-facts-test`, and PR checks.
#   - a release whose change files declare `Data-Migration:` -> data_migration true,
#     its notes carry "### Data migration";
#   - the next release, without one -> data_migration false;
#   - quotes and backslashes in a declaration stay valid JSON;
#   - bump-queued (the release queue's question): only a first-parent commit
#     by fts-forge[bot] with `Release-Queue: bump` since the last tag asks.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
cd "$t"
git init -q -b main
git config user.name test; git config user.email test@example.invalid
# A person, whatever identity the caller's environment carries.
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid
mkdir -p scripts .changes
cp "$here/release.sh" scripts/
change() { printf -- '---\n"studio-architect" = "%s"\n---\n\n%s\n' "$2" "$3" >".changes/$1.md"; git add -A; git commit -qm "$1"; }
change base patch "Fixed a thing (studio)"
git tag v26.1.0
change setlist-column minor $'Added a column to setlists (studio)\n\nData-Migration: setlists gain a "key" column (a \\ backslash), not undone by a rollback'
git tag v26.1.1
change later patch "Fixed a later thing (studio)"
git tag v26.1.2
fail=0
facts="$(bash scripts/release.sh facts v26.1.1)"
python3 -c 'import json,sys; f=json.loads(sys.argv[1]); assert f["data_migration"] is True and f["repo"]=="studio" and "key" in f["migrations"][0], f' "$facts" \
    && echo "ok   v26.1.1 declares its migration: $facts" || { echo "FAIL v26.1.1: $facts" >&2; fail=1; }
notes="$(bash scripts/release.sh notes v26.1.1)"
grep -qx '### Data migration' <<<"$notes" \
    && echo "ok   v26.1.1's notes carry ### Data migration" || { echo "FAIL v26.1.1's notes: $notes" >&2; fail=1; }
facts="$(bash scripts/release.sh facts v26.1.2)"
python3 -c 'import json,sys; f=json.loads(sys.argv[1]); assert f["data_migration"] is False and f["migrations"]==[], f' "$facts" \
    && echo "ok   v26.1.2 migrates nothing: $facts" || { echo "FAIL v26.1.2: $facts" >&2; fail=1; }
# bump-queued
person() { git commit -q --allow-empty -m "$1" "${@:2}"; }
person $'feat: a person\n\nRelease-Queue: bump'          # the trailer, but not the bot: ignored
if bash scripts/release.sh bump-queued HEAD >/dev/null; then echo "FAIL bump-queued took a person's commit" >&2; fail=1
else echo "ok   bump-queued ignores a person's commit with the trailer"; fi
GIT_AUTHOR_EMAIL='345159799+fts-forge[bot]@users.noreply.github.com' person "build(deps): a bot commit without the trailer"
if bash scripts/release.sh bump-queued HEAD >/dev/null; then echo "FAIL bump-queued took a bot commit without the trailer" >&2; fail=1
else echo "ok   bump-queued ignores a bot commit without the trailer"; fi
GIT_AUTHOR_EMAIL='345159799+fts-forge[bot]@users.noreply.github.com' person $'build(deps): architect v0.12.5\n\nRelease-Queue: bump'
bot="$(git rev-parse HEAD)"
person "fix: a later person's commit"
if [ "$(bash scripts/release.sh bump-queued HEAD)" = "$bot" ]; then echo "ok   bump-queued finds the bot's bump since v26.1.2"
else echo "FAIL bump-queued missed the bot's bump" >&2; fail=1; fi
git tag v26.1.3
person "fix: after the release"
if bash scripts/release.sh bump-queued HEAD >/dev/null; then echo "FAIL bump-queued asks again after a release" >&2; fail=1
else echo "ok   bump-queued does not ask again once released"; fi
exit "$fail"
