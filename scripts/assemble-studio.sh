#!/usr/bin/env bash
# Assemble FastTrackStudios/studio: session's history at the root, and
# daw, processor, signal and patchbay each with its history under its own
# directory (`daw/`, `processor/`, ...).
#
# Re-runnable. The rewrite is deterministic — the same source commits come
# out as the same studio commits — so running it again after the source
# repos have moved on fetches only their new commits and merges them on
# top of the earlier import, as an ordinary merge. Stops at the first
# merge that conflicts, for a person to finish.
#
# Each imported repo's root Cargo.toml / Cargo.lock become
# `<repo>/Cargo.workspace.{toml,lock}`: left as `Cargo.toml` they would be
# workspace roots nested inside studio's, which cargo refuses. Their
# contents are folded into studio's root manifest by hand.
#
# Tags are not imported: the old repos keep theirs (and stay archived, so
# git deps pinned to them keep resolving).
#
#   FTS=/run/media/Development/fts scripts/assemble-studio.sh
set -euo pipefail

FTS=${FTS:-/run/media/Development/fts}
OUT=${OUT:-$FTS/studio}
WORK=${WORK:-$OUT/.git/assemble}

# Which branch of each source repo studio follows.
ROOT=session
ROOT_BRANCH=${ROOT_BRANCH:-frame-mixer}
declare -A BRANCH=(
    [daw]=${DAW_BRANCH:-fx-key}
    [processor]=${PROCESSOR_BRANCH:-builtin-fx}
    [signal]=${SIGNAL_BRANCH:-rig-dump}
    [patchbay]=${PATCHBAY_BRANCH:-main}
)
ORDER=(daw processor signal patchbay)
# Optional trailer for the merge commits (e.g. a session link).
TRAILER=${TRAILER:-}
msg() { if [[ -n $TRAILER ]]; then printf '%s\n\n%s' "$1" "$TRAILER"; else printf '%s' "$1"; fi; }

filter_repo() { nix shell nixpkgs#git-filter-repo -c git filter-repo "$@"; }

if [[ ! -d $OUT/.git ]]; then
    git clone --no-tags --single-branch --branch "$ROOT_BRANCH" "$FTS/$ROOT" "$OUT"
    git -C "$OUT" branch -m "$ROOT_BRANCH" main
    git -C "$OUT" remote rename origin "$ROOT"
else
    if [[ -n $(git -C "$OUT" status --porcelain --untracked-files=no) ]]; then
        echo "studio has uncommitted changes; commit or stash them first" >&2
        exit 1
    fi
    git -C "$OUT" fetch --no-tags "$ROOT" "$ROOT_BRANCH"
    git -C "$OUT" merge --no-edit -m "$(msg "Merge $ROOT $ROOT_BRANCH")" FETCH_HEAD
fi

mkdir -p "$WORK"
for repo in "${ORDER[@]}"; do
    branch=${BRANCH[$repo]}
    tmp=$WORK/$repo
    rm -rf "$tmp"
    git clone --quiet --no-local --no-tags --single-branch --branch "$branch" "$FTS/$repo" "$tmp"
    filter_repo --quiet --force --source "$tmp" --target "$tmp" \
        --path-rename Cargo.toml:Cargo.workspace.toml \
        --path-rename Cargo.lock:Cargo.workspace.lock \
        --path-rename ":$repo/"
    git -C "$OUT" fetch --quiet --no-tags "$tmp" "HEAD:refs/import/$repo"
    if git -C "$OUT" merge-base --is-ancestor "refs/import/$repo" HEAD; then
        echo "$repo: nothing new"
    else
        git -C "$OUT" merge --allow-unrelated-histories --no-edit \
            -m "$(msg "Import $repo ($branch) into $repo/")" "refs/import/$repo"
    fi
    rm -rf "$tmp"
done
echo "studio assembled at $OUT ($(git -C "$OUT" rev-parse --short HEAD))"
