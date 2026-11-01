#!/usr/bin/env bash
# Make a worktree of studio complete, as the main checkout is: what git
# does not carry but the build and the app need.
#
#   assets   linked from the main checkout's .assets/ clones (a
#            copy-on-write clone of each, nothing fetched or copied:
#            scripts/assets.sh says why it is not a symlink). The guitar
#            rig embeds its captures, so without them `guitar` and every
#            crate above it compile different inputs from the main
#            checkout's (no kache hits) into an app with no captures.
#   faces    the plugin faces (`just faces` into
#            features/ui/plugin-faces/generated, not in git): the window
#            and the guitar rig read them at run time. Generated from this
#            worktree's own sources, in the background — the build does not
#            need them, so the checkout does not wait. kache serves the
#            generator from any earlier worktree's build.
#   .env     a symlink to the main checkout's (the dev shell sources it):
#            one file of local values, not copies that drift.
#   .envrc   allowed for direnv when it is the main checkout's, byte for
#            byte (as `cd` into the main checkout already is).
#
# Runs from prek's post-checkout hook (prek.toml) on a NEW worktree only —
# git gives the previous HEAD as all zeros for `git worktree add` — and is
# a no-op on every other checkout. By hand: `just worktree-setup` (any
# checkout, the faces regenerated in the foreground). Idempotent, quiet when
# all is well, and never fails the checkout: problems are warnings.
set -uo pipefail

force=
[[ ${1:-} == --force ]] && force=1
if [[ -z $force ]]; then
    [[ ${PRE_COMMIT_FROM_REF:-x} =~ ^0+$ ]] || exit 0
    [[ ${PRE_COMMIT_CHECKOUT_TYPE:-1} == 1 ]] || exit 0
fi

warn() { echo "worktree-setup: $*" >&2; }

top=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$top" || exit 0
main=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
gitdir=$(git rev-parse --absolute-git-dir)

# A worktree inside the main checkout (.claude/worktrees/* among them)
# picks up the main checkout's .cargo/config.toml as well as its own, and
# cargo concatenates their rustflags: every flag twice, so every crate
# keys differently in kache from the main checkout and from every
# worktree outside it.
case $top/ in
    "$main"/?*) warn "$top is inside the main checkout: cargo reads both .cargo/config.toml files (rustflags twice), so kache shares nothing with other checkouts. Put worktrees outside it." ;;
esac

# Assets. A repo the main checkout has not cloned is a warning.
ASSETS_LOCAL=1 bash scripts/assets.sh || warn "linking the assets failed"

if [[ $top != "$main" ]]; then
    if [[ -e $main/.env && ! -e .env && ! -L .env ]]; then
        ln -s "$main/.env" .env
    fi
    if command -v direnv >/dev/null && [[ -f .envrc ]] && cmp -s .envrc "$main/.envrc"; then
        direnv allow . >/dev/null 2>&1 || true
    fi
fi

# The plugin faces, written to a fresh directory and swapped in, so an
# interrupted run never leaves a half-written set behind. In the dev shell
# when outside it (its toolchain and flags are the ones the cache knows).
faces=features/ui/plugin-faces/generated
gen_faces() {
    local tmp="$faces.tmp.$$" run=()
    if [[ -z ${IN_NIX_SHELL:-} ]] && command -v nix >/dev/null; then
        run=(nix develop "$top" --command)
    fi
    rm -rf "$tmp"
    if timeout 1800 ${run[@]+"${run[@]}"} cargo run -q --release -p plugin-faces -- "$tmp"; then
        rm -rf "$faces" && mv "$tmp" "$faces"
    else
        rm -rf "$tmp"
        return 1
    fi
}
if [[ -n $force ]]; then
    gen_faces >/dev/null || warn "generating the plugin faces failed: run \`just faces\`"
elif [[ ! -e $faces/rig-faces/faces.json ]]; then
    log="$gitdir/worktree-setup.log"
    lock="$gitdir/worktree-setup.faces.lock"
    if mkdir "$lock" 2>/dev/null; then
        (
            trap '' HUP
            trap 'rmdir "$lock"' EXIT
            gen_faces || echo "worktree-setup: generating the plugin faces failed: run \`just faces\`"
        ) >"$log" 2>&1 </dev/null &
        disown
        echo "worktree-setup: generating the plugin faces in the background (log: $log)"
    fi
fi
exit 0
