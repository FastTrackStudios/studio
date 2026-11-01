#!/usr/bin/env bash
# The large binaries kept out of this repository (assets.txt: the private
# studio-assets, rig-models and mic-models repos), cloned into
# .assets/<repo> and each listed file or directory linked into place.
# `just assets` runs this; re-run to update. Clones over https so gh's
# credential helper authenticates (fleet hosts have no GitHub ssh key).
#
#   ASSETS_OFFLINE=1  never touch the network: link what .assets/ holds; a
#                     repo that is not there fails (CI's PR runs, which get
#                     no credential)
#   ASSETS_LOCAL=1    never touch the network either, but a repo that is
#                     not there is a warning and its entries are skipped
#                     (scripts/worktree-setup.sh: never fail a checkout)
#
# A worktree with no clone of its own starts from the main checkout's: a
# copy-on-write clone of `<main>/.assets/<repo>` (reflink: btrfs, XFS,
# APFS), so no bytes are copied and nothing is fetched. It is a real
# directory inside the worktree on purpose. The guitar rig's build script
# embeds every capture by its canonical path relative to OUT_DIR
# (features/rigs/guitar/build.rs), so a symlink to the main checkout's
# .assets would put that checkout's location in the generated source, and
# `guitar` and everything above it (the engine, the window, the app) would
# key differently in kache from the main checkout's build — and a path
# through a symlink is one git cannot answer for, so cargo-rail could record
# no evidence. Only where reflinks are not possible (another filesystem) is
# it a symlink, with a warning.
#
# bash 3.2 (macOS's /bin/bash, a CI runner without nix): no `declare -A`.
set -euo pipefail

top=$(git rev-parse --show-toplevel)
cd "$top"
# The main checkout: the one holding the common .git (`<main>/.git`).
main=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
offline=${ASSETS_OFFLINE:-}
local_only=${ASSETS_LOCAL:-}

warn() { echo "assets: $*" >&2; }

# A copy-on-write clone of the main checkout's clone of $1, or a symlink.
seed() {
    local repo=$1 src="$main/.assets/$1" tmp
    [[ $main != "$top" && -d $src/.git ]] || return 1
    mkdir -p .assets
    tmp=".assets/.$repo.seed.$$"
    rm -rf "$tmp"
    # GNU cp's reflink, else macOS's clonefile (`cp -c`); never a full copy.
    if cp -a --reflink=always "$src" "$tmp" 2>/dev/null \
        || { [[ $(uname -s) == Darwin ]] && cp -cRp "$src" "$tmp" 2>/dev/null; }; then
        mv "$tmp" ".assets/$repo"
    else
        rm -rf "$tmp"
        ln -s "$src" ".assets/$repo"
        warn "$repo: linked to $src (no reflinks here): guitar's build keys will differ from the main checkout's"
    fi
}

n=0
repos=0
fetched=" "
skipped=" "
while read -r repo from to; do
    [[ -z $repo || $repo == \#* ]] && continue
    to=${to:-$from}
    [[ $skipped == *" $repo "* ]] && continue
    if [[ $fetched != *" $repo "* ]]; then
        [[ -e .assets/$repo ]] || seed "$repo" || true
        if [[ -n $offline || -n $local_only ]]; then
            if [[ ! -d .assets/$repo/.git ]]; then
                if [[ -n $offline ]]; then
                    warn "ASSETS_OFFLINE: .assets/$repo is not cloned"
                    exit 1
                fi
                warn "$repo: not cloned here or in $main/.assets — run \`just assets\` in the main checkout; its files are not linked"
                skipped="$skipped$repo "
                continue
            fi
        elif [[ -d .assets/$repo/.git ]]; then
            git -C ".assets/$repo" pull --ff-only --quiet
        else
            git clone --quiet "https://github.com/FastTrackStudios/$repo.git" ".assets/$repo"
        fi
        fetched="$fetched$repo "
        repos=$((repos + 1))
    fi
    mkdir -p "$(dirname "$to")"
    # A relative link (`../../../../.assets/rig-models/guitar`), the same
    # text in every checkout. The guitar's build script watches the folder
    # holding its `models` link, and kache keys a watched folder by what it
    # holds, the link's target text among it: an absolute target named the
    # checkout, so `guitar` and everything above it keyed differently in
    # every worktree.
    target="$(dirname "$to" | sed 's|[^/][^/]*|..|g')/.assets/$repo/$from"
    # Only when it isn't already that link: re-linking moves the parent
    # directory's mtime, and build scripts that watch it (FTS Mic's
    # `models`) then rebuild — and everything above them.
    [ "$(readlink "$to" 2>/dev/null)" = "$target" ] || ln -sfn "$target" "$to"
    n=$((n + 1))
done <assets.txt
if [[ -z $local_only ]]; then
    echo "linked $n assets from $repos repos"
fi
