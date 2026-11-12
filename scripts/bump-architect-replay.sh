#!/usr/bin/env bash
# The bump script's test: replay real Architect bumps and check that the
# script writes what was merged then (`just bump-architect-test`).
#
#   1. studio#457 (architect v0.12.3 -> v0.12.4, keyflow v0.5.2 -> v0.5.3,
#      keyflow found by `--keyflow auto`): a mechanical bump. The result
#      must equal the merged commit (Cargo.toml, Cargo.lock and
#      .config/duplicate-deps.toml's note), apart from the change file's
#      name and wording: there must be exactly one new one, for
#      studio-architect, patch.
#   2. the same base asked for v0.12.4 with keyflow kept at v0.5.2 (which
#      pins v0.12.3): it must exit 4, waiting on keyflow, and change nothing.
#   3. v0.11.2 -> v0.12.0 (Architect moved the editor revision Studio
#      follows): it must exit 3 and name the editor.
#   4. A PATCH bump that skips Keyflow (the www spelling, keyflow#68): this
#      checkout's HEAD (with the www [patch] table) and keyflow pinned at
#      keyflow#68's www commit, on architect v0.12.4, asked for v0.12.5. It
#      must exit 0 and leave keyflow alone; Cargo.lock must then hold ONE
#      architect source, v0.12.5 on the plain URL (task@main's v0.10.0
#      aside), none on www; and on the base the architect sources (the
#      package ids kache keys on) must be exactly main's: the www table
#      adds no new source.
# Not hermetic: it reads GitHub (tags, manifests, locks, compare), so it is
# a manual check (`just bump-architect-test`), not a CI job.
#
# Each case checks its base out in a scratch worktree and runs THIS
# checkout's scripts (bump-architect.py, check-duplicate-deps.py) there.
# Needs the network (the manifests, keyflow's tags, cargo's git fetches);
# compiles nothing.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
fail=0
scripts=(bump-architect.py check-duplicate-deps.py)

# replay BASE TAG EXPECT_STATUS WANT [ARGS...]
#   WANT: the merged commit (status 0), a string the report names (3), - (4)
replay() {
    local base="$1" tag="$2" expect="$3" want="$4"
    shift 4
    git -C "$root" cat-file -e "$base^{commit}" 2>/dev/null || git -C "$root" fetch -q origin main
    local scratch status=0 s
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/bump-replay.XXXXXX")"
    # No hooks, no LFS: only the manifests and the scripts matter.
    GIT_LFS_SKIP_SMUDGE=1 git -C "$root" -c core.hooksPath=/dev/null worktree add -q --detach "$scratch" "$base"
    for s in "${scripts[@]}"; do cp "$root/scripts/$s" "$scratch/scripts/$s"; done
    (cd "$scratch" && python3 scripts/bump-architect.py "$tag" "$@" --report "$scratch.report") >"$scratch.log" 2>&1 || status=$?
    # Back to the base's own scripts, so only the bump shows in the diff.
    git -C "$scratch" checkout -q "$base" -- scripts 2>/dev/null || true
    git -C "$scratch" clean -fdq -- scripts

    if [ "$status" -ne "$expect" ]; then
        echo "FAIL $base -> $tag $*: exit $status, expected $expect" >&2
        cat "$scratch.log" >&2
        fail=1
    elif [ "$expect" -eq 4 ]; then
        if git -C "$scratch" status --porcelain | grep -q .; then
            echo "FAIL $base -> $tag $*: waiting on keyflow, but it changed files:" >&2
            git -C "$scratch" status --short >&2
            fail=1
        else
            echo "ok   $base -> $tag $*: waiting on keyflow, nothing changed"
        fi
    elif [ "$expect" -eq 3 ]; then
        if grep -q "$want" "$scratch.report" 2>/dev/null; then
            echo "ok   $base -> $tag $*: needs a person ($want moved)"
        else
            echo "FAIL $base -> $tag $*: the report does not name $want" >&2
            cat "$scratch.report" "$scratch.log" >&2 2>/dev/null || true
            fail=1
        fi
    else
        local new
        new="$(git -C "$scratch" status --porcelain -- .changes | sed -n 's/^?? //p')"
        if [ "$(wc -w <<<"$new")" -ne 1 ] || ! grep -qx '"studio-architect" = "patch"' "$scratch/$new"; then
            echo "FAIL $base -> $tag $*: expected one new change file for studio-architect (patch), got: ${new:-none}" >&2
            fail=1
        fi
        rm -f "$scratch/$new"
        if git -C "$scratch" diff --quiet "$want" -- . ':(exclude).changes'; then
            echo "ok   $base -> $tag $*: identical to $want (change file: $new)"
        else
            echo "FAIL $base -> $tag $*: the result differs from $want:" >&2
            git -C "$scratch" diff --stat "$want" -- . ':(exclude).changes' >&2
            git -C "$scratch" diff "$want" -- . ':(exclude).changes' | head -80 >&2
            fail=1
        fi
    fi
    git -C "$root" worktree remove --force "$scratch" >/dev/null 2>&1 || true
    rm -rf "$scratch" "$scratch.log" "$scratch.report"
}

# studio#456 merged (architect v0.12.3, keyflow v0.5.2) -> studio#457 merged
replay a7ec2ad9cd484be12582c0d79e639c1ee28416d3 v0.12.4 0 3802588ff0e56b7adaa95d89edaf7b89cc4195d8 --keyflow auto --no-facts
replay a7ec2ad9cd484be12582c0d79e639c1ee28416d3 v0.12.4 4 - --keyflow keep --no-facts
# studio#387's line (architect v0.11.2) -> v0.12.0, which moved the editor
# revision (81451ea -> 6817dc2): studio#401 moved it by hand
replay d59b4828d297c341dd8e0bcd522e39ef15f48eb3 v0.12.0 3 fasttrackstudios/editor --keyflow v0.5.0 --no-lock --no-change-file --no-facts

# 4. A patch bump that skips keyflow.
KEYFLOW_WWW=2b03d43566b9633ef711020a18f4c720aaee490e # keyflow#68: Architect on the www spelling
www_case() {
    local scratch status=0 s before after
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/bump-replay.XXXXXX")"
    GIT_LFS_SKIP_SMUDGE=1 git -C "$root" -c core.hooksPath=/dev/null worktree add -q --detach "$scratch" HEAD
    for s in "${scripts[@]}"; do cp "$root/scripts/$s" "$scratch/scripts/$s"; done
    grep -q '^\[patch."https://www.github.com/FastTrackStudios/architect"\]' "$scratch/Cargo.toml" \
        || { echo "FAIL www: HEAD has no www [patch] table" >&2; fail=1; return; }
    main_sources="$(git -C "$root" show HEAD:Cargo.lock | grep -o 'git+https://[a-z.]*github.com/FastTrackStudios/architect?tag=v[0-9.]*' | sort -u)"
    # The base: keyflow on its www commit (pinned by rev, as no release has it yet).
    sed -i -E "s|(git = \"https://github.com/FastTrackStudios/keyflow\"), tag = \"v[0-9.]+\"|\1, rev = \"$KEYFLOW_WWW\"|" "$scratch/Cargo.toml"
    (cd "$scratch" && cargo metadata --format-version 1 >/dev/null 2>"$scratch.log") || { echo "FAIL www: the base does not resolve" >&2; cat "$scratch.log" >&2; fail=1; return; }
    before="$(grep -o 'git+https://[a-z.]*github.com/FastTrackStudios/architect?tag=v[0-9.]*' "$scratch/Cargo.lock" | sort -u)"
    if [ "$before" = "$main_sources" ]; then
        echo "ok   www base: keyflow on www, the architect sources are the tree's own, unchanged (kache keys the same)"
    else
        echo "FAIL www base: architect sources differ from the tree's own:" >&2; diff <(echo "$main_sources") <(echo "$before") >&2; fail=1
    fi
    if (cd "$scratch" && python3 scripts/check-duplicate-deps.py >/dev/null); then
        echo "ok   www base: check-duplicate-deps passes"
    else
        echo "FAIL www base: check-duplicate-deps" >&2; fail=1
    fi
    git -C "$scratch" -c user.name=t -c user.email=t@t -c core.hooksPath=/dev/null commit -qam "base: keyflow on www"
    (cd "$scratch" && python3 scripts/bump-architect.py v0.12.5 --keyflow auto --no-facts --report "$scratch.report") >"$scratch.log" 2>&1 || status=$?
    after="$(grep -o 'git+https://[a-z.]*github.com/FastTrackStudios/architect?tag=v[0-9.]*' "$scratch/Cargo.lock" | sort -u)"
    if [ "$status" != 0 ]; then
        echo "FAIL www patch bump: exit $status" >&2; cat "$scratch.log" >&2; fail=1
    elif git -C "$scratch" diff --quiet HEAD -- Cargo.toml && true; then
        echo "FAIL www patch bump: Cargo.toml did not move" >&2; fail=1
    elif ! git -C "$scratch" diff HEAD -- Cargo.toml | grep -q "^-.*keyflow.*$KEYFLOW_WWW" ; then
        if [ "$after" = "$(printf '%s\n' "${main_sources//v0.12.4/v0.12.5}" | sort -u)" ]; then
            echo "ok   www patch bump v0.12.4 -> v0.12.5: exit 0, keyflow untouched, one architect source (v0.12.5), none on www"
        else
            echo "FAIL www patch bump: architect sources after:" >&2; echo "$after" >&2; fail=1
        fi
    else
        echo "FAIL www patch bump: it moved keyflow" >&2; fail=1
    fi
    git -C "$root" worktree remove --force "$scratch" >/dev/null 2>&1 || true
    rm -rf "$scratch" "$scratch.log" "$scratch.report"
}
www_case

# 5. Facts unknown (architect v0.12.4 predates release-facts.json): exit 3,
#    and the change file says `Data-Migration: … unknown`, so Studio's own
#    release-facts.json declares it and infra never auto-merges that node.
facts_case() {
    local scratch status=0 cf
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/bump-replay.XXXXXX")"
    GIT_LFS_SKIP_SMUDGE=1 git -C "$root" -c core.hooksPath=/dev/null worktree add -q --detach "$scratch" a7ec2ad9cd484be12582c0d79e639c1ee28416d3
    for s in "${scripts[@]}"; do cp "$root/scripts/$s" "$scratch/scripts/$s"; done
    (cd "$scratch" && python3 scripts/bump-architect.py v0.12.4 --keyflow auto --no-lock) >"$scratch.log" 2>&1 || status=$?
    cf="$(git -C "$scratch" status --porcelain -- .changes | sed -n 's/^?? //p')"
    if [ "$status" = 3 ] && [ -n "$cf" ] && grep -q '^Data-Migration: architect v0.12.4: unknown' "$scratch/$cf"; then
        echo "ok   facts unknown: exit 3, the change file declares an unknown migration"
    else
        echo "FAIL facts unknown: exit $status, change file ${cf:-none}" >&2; cat "$scratch.log" >&2; fail=1
    fi
    git -C "$root" worktree remove --force "$scratch" >/dev/null 2>&1 || true
    rm -rf "$scratch" "$scratch.log"
}
facts_case

exit "$fail"
