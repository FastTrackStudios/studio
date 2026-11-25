#!/usr/bin/env bash
# Replay Studio commits that touched Frame onto this repo, until Studio
# depends on Frame by tag (FastTrackStudios/studio#373).
#
#   scripts/frame-catch-up.sh <studio checkout> [<from>..<to>] [--dry-run]
#
# Frame's history came out of Studio with git filter-repo; every carried
# commit ends with a `Studio-commit: FastTrackStudios/studio@<sha>` trailer.
# With no range, this replays what Studio's `origin/main` has after the
# newest such trailer on HEAD. Each commit is cut to Frame's paths, renamed
# into this repo's layout (the same mapping the import used), applied with
# `git am -3` and given the trailer. Commits that touch no Frame path are
# skipped. A conflict stops at `git am`: resolve, `git am --continue`, rerun.
set -euo pipefail

studio=${1:?usage: frame-catch-up.sh <studio checkout> [<from>..<to>] [--dry-run]}
shift
arg="" dry=false
for a in "$@"; do
  case $a in
    --dry-run) dry=true ;;
    *) arg=$a ;;
  esac
done

# Studio's Frame paths, and where each lands here.
paths=(crates/frame apps/frame docs/frame docs/frame-themes.md xtask/frame
       features/ui/plugin-faces scripts/face_gallery.py)
rename() {
  sed -E \
    -e '/^(diff --git|--- |\+\+\+ |rename (from|to) |copy (from|to) )/{
          s#(^|[ /])docs/frame/#\1docs/#g
          s#(^|[ /])xtask/frame/#\1xtask/#g
          s#(^|[ /])features/ui/plugin-faces/#\1examples/plugin-faces/#g
        }'
}

if [ -z "$arg" ]; then
  last=$(git log --format=%B | sed -nE 's#^Studio-commit: FastTrackStudios/studio@([0-9a-f]+)$#\1#p' | head -n1 || true)
  [ -n "$last" ] || { echo "no Studio-commit trailer on HEAD; pass a range" >&2; exit 1; }
  git -C "$studio" fetch -q origin main
  arg="$last..origin/main"
fi

commits=$(git -C "$studio" rev-list --reverse --no-merges "$arg" -- "${paths[@]}")
[ -n "$commits" ] || { echo "nothing to replay in $arg"; exit 0; }

for c in $commits; do
  echo "$(git -C "$studio" log -1 --format='%h %s' "$c")"
  $dry && continue
  git -C "$studio" format-patch -1 --stdout --no-signature "$c" -- "${paths[@]}" \
    | rename \
    | git am -3 --keep-non-patch
  msg=$(git log -1 --format=%B | sed -E 's/(^|[^A-Za-z0-9/_-])#([0-9]+)/\1FastTrackStudios\/studio#\2/g')
  printf '%s\n\nStudio-commit: FastTrackStudios/studio@%s\n' "$msg" "$(git -C "$studio" rev-parse "$c")" \
    | git commit -q --amend --no-verify -F -
done
