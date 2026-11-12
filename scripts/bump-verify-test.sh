#!/usr/bin/env bash
# shellcheck disable=SC2015 # echo cannot fail: A && ok || fail is if-then-else here
# bump-verify.py against fixtures (`just bump-verify-test`, PR checks).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
t="$(mktemp -d)"
trap 'rm -rf "$t"' EXIT
sha=$(printf 'a%.0s' {1..40})
R=repos_FastTrackStudios_studio
pr() { # <head ref> <user> <base> <head repo>
    printf '[{"number":7,"merge_commit_sha":"%s","merged_at":"2026-11-12T00:00:00Z","base":{"ref":"%s"},"head":{"ref":"%s","repo":{"full_name":"%s"}},"user":{"login":"%s"}}]' \
        "$sha" "$3" "$1" "$4" "$2" > "$t/${R}_commits_${sha}_pulls.json"
}
files() { jq -n --args '$ARGS.positional | map({filename: .})' "$@" > "$t/${R}_pulls_7_files_per_page=100&page=1.json"; }
fail=0
check() { # <want exit> <what>
    local got=0
    GITHUB_REPOSITORY=FastTrackStudios/studio python3 "$here/bump-verify.py" "$sha" --fixtures "$t" >/dev/null || got=$?
    [ "$got" = "$1" ] && echo "ok   $2" || { echo "FAIL $2 (exit $got)" >&2; fail=1; }
}
pr bump/architect-v0.12.5 'fts-forge[bot]' main FastTrackStudios/studio
files Cargo.toml Cargo.lock .config/duplicate-deps.toml .changes/studio-architect-on-architect-1a2b.md
check 0 "a real bot bump"
pr feat/something 'fts-forge[bot]' main FastTrackStudios/studio
check 1 "an agent PR (fts-forge) from another branch, whatever its body says"
pr bump/architect-v0.12.5 'someone' main FastTrackStudios/studio
check 1 "a bump branch opened by a person"
pr bump/architect-v0.12.5 'fts-forge[bot]' main someone/studio
check 1 "a bump branch from a fork"
pr bump/architect-v0.12.5 'fts-forge[bot]' main FastTrackStudios/studio
files Cargo.toml Cargo.lock scripts/release.sh
check 1 "a bump that changes a script"
rm "$t/${R}_commits_${sha}_pulls.json"
check 1 "a commit that is no PR's merge"
exit "$fail"
