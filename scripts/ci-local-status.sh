#!/usr/bin/env bash
# The `ci/local` commit status: a full `just ci` that passed on a clean tree
# says so on GitHub, and the PR's plan job then skips the tests job for that
# exact tree (xtask/workflows/src/checks.rs, LOCAL_RUN).
#
#   ci-local-status.sh record <sha>   after a full `just ci` passed: note it
#   ci-local-status.sh publish        post every noted pass GitHub has the
#                                     commit for (`just push` runs this)
#
# The status names the commit's TREE, so CI can check that what passed is
# byte for byte what it would test. GitHub only takes a status for a commit
# it has, so a pass is noted under .git/ci-local/ and posted once pushed.
set -euo pipefail

dir="$(git rev-parse --git-common-dir)/ci-local"
platform="$(uname -s)-$(uname -m)"

repo() {
  local url
  url="$(git remote get-url origin)"
  url="${url%.git}"
  printf '%s\n' "${url#*github.com[:/]}"
}

case "${1:-}" in
record)
  sha="${2:?record <sha>: the commit the run started on}"
  if [ "$(git rev-parse HEAD)" != "$sha" ]; then
    echo "ci/local: HEAD moved during the run; nothing recorded"
    exit 0
  fi
  # Untracked files count: cargo builds a new tests/*.rs whether or not it
  # is committed, so a tree with one is not the commit's tree.
  if [ -n "$(git status --porcelain)" ]; then
    echo "ci/local: the tree has uncommitted or untracked changes, so what passed is not a commit; nothing recorded"
    exit 0
  fi
  mkdir -p "$dir"
  printf '%s %s %s\n' "$(git rev-parse "$sha^{tree}")" "$platform" "$(hostname -s)" > "$dir/$sha"
  echo "ci/local: recorded the pass for $(git rev-parse --short "$sha")"
  exec "$0" publish
  ;;
publish)
  [ -d "$dir" ] || exit 0
  r="$(repo)"
  for f in "$dir"/*; do
    [ -e "$f" ] || continue
    sha="$(basename "$f")"
    read -r tree plat host < "$f"
    if ! gh api "repos/$r/commits/$sha" --silent >/dev/null 2>&1; then
      echo "ci/local: $(git rev-parse --short "$sha" 2>/dev/null || echo "$sha") is not on GitHub yet; \`just push\` (or \`just ci-publish\` after pushing) posts it"
      continue
    fi
    gh api -X POST "repos/$r/statuses/$sha" \
      -f state=success -f context=ci/local \
      -f description="just ci passed tree $tree on $plat ($host)" --silent
    rm -f "$f"
    echo "ci/local: posted for $(git rev-parse --short "$sha" 2>/dev/null || echo "$sha")"
  done
  # Passes for commits that never got pushed: two weeks is plenty.
  find "$dir" -type f -mtime +14 -delete 2>/dev/null || true
  ;;
*)
  echo "usage: $0 record <sha> | publish" >&2
  exit 2
  ;;
esac
