#!/usr/bin/env bash
# Link the rig captures into the tree, for a RELEASE build on a CI runner.
#
#   scripts/ci/link-rig-models.sh <path to a rig-models checkout>
#
# The guitar rig ships its captures and IRs inside the binary, and its
# build.rs refuses a release build without them ("default-config/models is
# empty: run `just assets`"). They live in the private rig-models repo, which
# `just assets` clones over SSH into .assets/ and links in place (assets.txt
# names the mapping). A CI runner has no SSH key for it and checks out a tree
# without them, so each build host keeps one clone of rig-models and the
# workflow links it with this script:
#
#   airlock          /Volumes/build-disk/development/rig-models
#   THEBATTLESHIP    /run/media/Development/ci-assets/rig-models
#
# Only the `rig-models` lines of assets.txt are linked (the other repo's
# lines are test fixtures a release does not need). Prints the clone's commit,
# so a build log says which captures it shipped. Fails, naming the path, when
# the clone is missing or holds no captures: a release must not ship a rig with
# none.
set -euo pipefail

src="${1:-${RIG_MODELS_DIR:-}}"
[ -n "$src" ] || { echo "usage: link-rig-models.sh <rig-models checkout>" >&2; exit 2; }
[ -d "$src" ] || { echo "ERROR: no rig-models checkout at $src (see the header of this script)" >&2; exit 1; }
src="$(cd "$src" && pwd)"
cd "$(dirname "$0")/../.."

n=0
while read -r repo from to; do
    [[ -z $repo || $repo == \#* ]] && continue
    [ "$repo" = rig-models ] || continue
    to="${to:-$from}"
    [ -d "$src/$from" ] || { echo "ERROR: $src/$from is not a directory" >&2; exit 1; }
    count="$(find -L "$src/$from" -type f \( -name '*.nam' -o -name '*.wav' \) | wc -l | tr -d ' ')"
    [ "$count" -gt 0 ] || { echo "ERROR: $src/$from holds no .nam/.wav captures" >&2; exit 1; }
    mkdir -p "$(dirname "$to")"
    # An empty real directory (a checkout that created it) gives way to the link.
    if [ -d "$to" ] && [ ! -L "$to" ]; then rmdir "$to"; fi
    ln -sfn "$src/$from" "$to"
    echo "linked $to -> $src/$from ($count captures)"
    n=$((n + 1))
done < assets.txt
[ "$n" -gt 0 ] || { echo "ERROR: assets.txt has no rig-models lines" >&2; exit 1; }
echo "rig-models at $(git -C "$src" rev-parse --short HEAD 2>/dev/null || echo 'unknown commit')"
