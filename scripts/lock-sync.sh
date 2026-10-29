#!/usr/bin/env bash
# The committed Cargo.lock, made from what is being committed — never by
# hand (the prek `lock-sync` hook, on a commit that stages a Cargo.toml).
#
# With the fts-dev link active (the `# >>> fts-dev link` [patch] block in
# Cargo.toml, stripped from git by the `fts-dev` clean filter), the local
# Cargo.lock resolves against local checkouts and is skip-worktree: it is
# not what main may hold. So this resolves the STAGED tree — its manifests
# already link-free — in a throwaway worktree, from the committed lock,
# and stages the lock cargo writes there. The working tree (Cargo.toml,
# Cargo.lock) is never touched: a running `dx serve` sees nothing.
#
# Without the link there is nothing to do: cargo keeps the lock in the
# working tree, and it is staged like any file.
set -euo pipefail

repo="$(git rev-parse --show-toplevel)"
cd "$repo"

if ! grep -q '^# >>> fts-dev link' Cargo.toml 2>/dev/null; then
    exit 0
fi

tree="$(git write-tree)"
commit="$(git commit-tree "$tree" -p HEAD -m 'lock-sync: the staged tree')"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/lock-sync.XXXXXX")"
cleanup() {
    git worktree remove --force "$scratch" >/dev/null 2>&1 || true
    rm -rf "$scratch"
}
trap cleanup EXIT

# The staged tree, checked out on its own: no LFS downloads (only the
# manifests matter), no hooks.
GIT_LFS_SKIP_SMUDGE=1 git worktree add --quiet --detach "$scratch" "$commit" >/dev/null

# Resolve there: offline from the caches first, the network only when a
# git dependency the committed lock pins was never fetched here.
resolve() {
    (cd "$scratch" && timeout 600 cargo metadata --format-version 1 "$@" >/dev/null)
}
if ! resolve --offline 2>/dev/null; then
    echo "lock-sync: not everything is cached; resolving online" >&2
    resolve
fi

if git diff --quiet --no-index -- <(git show :Cargo.lock) "$scratch/Cargo.lock" 2>/dev/null; then
    exit 0
fi
blob="$(git hash-object -w "$scratch/Cargo.lock")"
git update-index --cacheinfo "100644,$blob,Cargo.lock"
echo "lock-sync: staged Cargo.lock as the staged manifests resolve it" >&2
