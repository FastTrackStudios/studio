#!/usr/bin/env bash
# Studio Architect: Studio's distribution of Architect (apps/architect,
# docs/architect-distribution.md), built the way Architect builds its own
# app's node and web bundle (architect's xtask/src/dist.rs at the tag
# Studio pins), from this checkout:
#
#   scripts/studio-architect.sh node            the headless node; prints node=<path>
#   scripts/studio-architect.sh web OUT         the web bundle into OUT; prints web=<OUT>
#   scripts/studio-architect.sh check           `cargo check` of the node (PR CI)
#   scripts/studio-architect.sh release TAG DIR the release's tarballs into DIR:
#       studio-architect-node-TAG-<arch>-<os>.tar.gz (+ .sha256)
#           studio-architect                the node
#           studio-island/                  its island (#445), from `just island`
#                                           at this commit (required)
#       studio-architect-web-TAG.tar.gz              (+ .sha256)
#
# The node is `--no-default-features` (no window: no Blitz, wgpu or
# wayland) with `--config target.<host>.rustflags=["--cfg","no_workspace_hack"]`,
# flag for flag what `cargo xtask dist node` passes: Architect's
# workspace-hack would pull the GUI stack's features back in. Cargo joins
# that array to .cargo/config.toml's table for the host (the fleet's, the
# same in Architect), so on x86_64 Linux the node is linked with mold and
# built for x86-64-v3 (AVX2, Haswell and later), as Architect's own
# release node is, with the same kache keys; on a Mac the flag stands
# alone (no table for aarch64-apple-darwin).
#
# The web bundle is Architect's web app (one wasm for every distribution)
# with this distribution's `/app/distribution.json` (what the node's
# `distribution` prints): Architect's scripts/web-bundle.sh, run from an
# Architect checkout at the commit Cargo.lock pins (ARCHITECT_SRC, or one
# cloned under the cache dir).
#
# Environment:
#   PROFILE                  cargo profile (default release)
#   CARGO_TARGET_DIR         the node's target dir (default this repo's)
#   STUDIO_ARCHITECT_CACHE   where the Architect checkout and its target dir
#                            live (default <target dir>/studio-architect)
#   ARCHITECT_SRC            an Architect checkout at the pinned commit
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
profile="${PROFILE:-release}"

host() { rustc -vV | sed -n 's/^host: //p'; }

target_dir() {
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        echo "$CARGO_TARGET_DIR"
    else
        cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
    fi
}

node_args() {
    printf '%s\n' --locked -p studio-architect --bin studio-architect --no-default-features \
        --config "target.$(host).rustflags=[\"--cfg\",\"no_workspace_hack\"]"
}

node() {
    local args=()
    while IFS= read -r a; do args+=("$a"); done < <(node_args)
    cargo build --profile "$profile" "${args[@]}" >&2
    local dir="$profile"
    [ "$profile" = dev ] && dir=debug
    local bin
    bin="$(target_dir)/$dir/studio-architect"
    [ -x "$bin" ] || { echo "studio-architect: no node at $bin" >&2; exit 1; }
    # It is the node of this distribution, and says so.
    "$bin" serve --help >/dev/null
    "$bin" distribution | grep -q '"bundle_id": "app.fasttrackstudio.studio-architect"' \
        || { echo "studio-architect: $bin is not Studio Architect" >&2; exit 1; }
    echo "node=$bin"
}

check() {
    local args=()
    while IFS= read -r a; do args+=("$a"); done < <(node_args)
    cargo check "${args[@]}" --all-targets
}

# The Architect checkout at the commit Cargo.lock pins for architect-desktop.
architect_src() {
    local source tag rev
    source="$(awk '/^name = "architect-desktop"$/{getline; getline; print; exit}' Cargo.lock)"
    tag="$(sed -n 's/.*?tag=\([^#]*\)#.*/\1/p' <<<"$source")"
    rev="$(sed -n 's/.*#\([0-9a-f]*\)".*/\1/p' <<<"$source")"
    [ -n "$tag" ] && [ -n "$rev" ] || { echo "studio-architect: Cargo.lock pins no architect-desktop tag ($source)" >&2; exit 1; }
    local src="${ARCHITECT_SRC:-}"
    if [ -z "$src" ]; then
        local cache="${STUDIO_ARCHITECT_CACHE:-$(target_dir)/studio-architect}"
        src="$cache/architect-$rev"
        if [ ! -d "$src/.git" ]; then
            rm -rf "$src"
            mkdir -p "$cache"
            git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$tag" https://github.com/FastTrackStudios/architect "$src" >&2
        fi
    fi
    local at
    at="$(git -C "$src" rev-parse HEAD)"
    [ "$at" = "$rev" ] || { echo "studio-architect: $src is at $at, Cargo.lock pins $rev ($tag)" >&2; exit 1; }
    echo "$src"
}

web() {
    local out="${1:?usage: studio-architect.sh web OUT}"
    local bin src json
    bin="$(node | sed -n 's/^node=//p')"
    src="$(architect_src)"
    json="$(mktemp)"
    "$bin" distribution > "$json"
    # Architect's own workspace and lockfile, in a target dir of its own.
    (
        export CARGO_TARGET_DIR="${STUDIO_ARCHITECT_CACHE:-$(target_dir)/studio-architect}/architect-target"
        ARCHITECT_DISTRIBUTION="$json" bash "$src/scripts/web-bundle.sh" "$out" >&2
    )
    rm -f "$json"
    grep -q '"name": "Studio Architect"' "$out/app/distribution.json" \
        || { echo "studio-architect: $out/app/distribution.json is not Studio Architect's" >&2; exit 1; }
    echo "web=$(cd "$out" && pwd)"
}

# The release's assets, named so they cannot be taken for Architect's own
# (architect-node-<tag>-x86_64-linux.tar.gz, architect-web-<v>.tar.gz).
release() {
    local tag="${1:?usage: studio-architect.sh release TAG DIR}" dir="${2:?usage: studio-architect.sh release TAG DIR}"
    mkdir -p "$dir"
    dir="$(cd "$dir" && pwd)"
    local arch os
    arch="$(uname -m)"
    case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) os="$(uname -s)" ;; esac
    local bin
    bin="$(node | sed -n 's/^node=//p')"
    local name="studio-architect-node-$tag-$arch-$os"
    rm -rf "${dir:?}/$name"
    mkdir -p "$dir/$name"
    cp "$bin" "$dir/$name/studio-architect"
    # Studio's island (#445) beside the binary, where the distribution reads
    # it (studio_architect::ISLAND_FOLDER): `just island`'s folder, which the
    # release job builds just before this, into the same target dir. A
    # release without it, or with one built from another commit, fails: a
    # stale island would ship code this tag does not hold.
    local island
    island="$(target_dir)/studio-island"
    local head
    head="$(git describe --always --dirty --abbrev=40)"
    if [ ! -f "$island/architect.lock.json" ]; then
        echo "studio-architect: no $island (run \`just island\`)" >&2
        exit 1
    fi
    case "$head" in
        *-dirty)
            echo "studio-architect: the tree has uncommitted changes ($head): a release ships only what a commit holds" >&2
            exit 1
            ;;
    esac
    local built
    built="$(cat "$island/BUILT_FROM" 2>/dev/null || echo 'an unknown tree')"
    if [ "$built" != "$head" ]; then
        echo "studio-architect: $island was built from $built, not $head: run \`just island\` on a clean tree" >&2
        exit 1
    fi
    mkdir -p "$dir/$name/studio-island"
    # What the node serves (the lock's files, the sheet among them, and the
    # lock) and the stamp; nothing else build-web left there.
    python3 - "$island" "$dir/$name/studio-island" <<'PY'
import json, shutil, sys, os
src, dst = sys.argv[1], sys.argv[2]
names = list(json.load(open(os.path.join(src, "architect.lock.json")))["files"])
for n in names + ["architect.lock.json", "BUILT_FROM"]:
    path = os.path.join(src, n)
    # As the node reads it: a regular file of its own, never a link.
    if "/" in n or os.path.islink(path) or not os.path.isfile(path):
        sys.exit(f"studio-architect: {path} is not a regular file (a link, a folder or missing): the node would refuse the island")
    shutil.copy2(path, os.path.join(dst, n))
PY
    tar -C "$dir" -czf "$dir/$name.tar.gz" "$name"
    rm -rf "${dir:?}/$name"
    local web="studio-architect-web-$tag"
    web "$dir/$web" >/dev/null
    tar -C "$dir" -czf "$dir/$web.tar.gz" "$web"
    rm -rf "${dir:?}/$web"
    (cd "$dir" && for f in "$name.tar.gz" "$web.tar.gz"; do sha256sum "$f" > "$f.sha256"; done)
    ls -l "$dir"
}

cmd="${1:-}"
shift || true
case "$cmd" in
    node) node ;;
    web) web "$@" ;;
    check) check ;;
    release) release "$@" ;;
    *)
        sed -n '2,12p' "$0" >&2
        exit 64
        ;;
esac
