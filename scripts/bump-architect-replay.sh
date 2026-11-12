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
replay a7ec2ad9cd484be12582c0d79e639c1ee28416d3 v0.12.4 0 3802588ff0e56b7adaa95d89edaf7b89cc4195d8 --keyflow auto
replay a7ec2ad9cd484be12582c0d79e639c1ee28416d3 v0.12.4 4 - --keyflow keep
# studio#387's line (architect v0.11.2) -> v0.12.0, which moved the editor
# revision (81451ea -> 6817dc2): studio#401 moved it by hand
replay d59b4828d297c341dd8e0bcd522e39ef15f48eb3 v0.12.0 3 fasttrackstudios/editor --keyflow v0.5.0 --no-lock --no-change-file

exit "$fail"
