#!/usr/bin/env bash
# What a macOS build of Studio links from outside macOS: nothing.
#
#   scripts/desktop/macos-libs.sh config        # -> the cargo config to build with
#   scripts/desktop/macos-libs.sh guard <bin>   # repoint, then refuse /nix/store
#
# `config` writes target/macos-libs/config.toml and prints its path: pass it
# to cargo (`cargo build --config "$(…/macos-libs.sh config)"`). It overrides
# libz-sys's build script (its `links = "z"`) for aarch64-apple-darwin: link
# `z` from a folder holding only Xcode's macOS SDK `libz.tbd`, whose install
# name is /usr/lib/libz.1.dylib. Without it, `-lz` is resolved through the dev
# shell's NIX_LDFLAGS, where freetype and fontconfig put nixpkgs' zlib
# (`/nix/store/…-zlib-1.3.2/lib/libz.dylib`), which exists on no other Mac:
# the v26.11.2 .dmg was refused for it (studio#444). nixpkgs' apple-sdk has
# no libz.tbd, so the stub is Xcode's, found with the system xcrun outside the
# shell's DEVELOPER_DIR/SDKROOT, its targets rewritten for the dev shell's
# older ld64, which calls Xcode 27's `arm64e.x1-macos` a "malformed file"
# and falls back to the store's zlib: the arm64, arm64e and x86_64 macOS
# targets older SDKs list. Only libz is in that folder: nothing else the
# link searches for is found there.
#
# `guard` is the .dmg's check (scripts/desktop/macos.sh), also run by CI's
# Apple builds on the binary it links: libiconv (the dev shell links its own)
# and libz are part of macOS, so a store copy of either is repointed at
# /usr/lib; anything else from /nix/store fails here, not at launch on
# another Mac. It edits <bin> in place: give it the copy that ships.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

case "${1:-}" in
config)
    sdk="$(env -u DEVELOPER_DIR -u SDKROOT /usr/bin/xcrun --sdk macosx --show-sdk-path)"
    [ -f "$sdk/usr/lib/libz.tbd" ] || { echo "ERROR: no libz.tbd in $sdk (is Xcode installed?)" >&2; exit 1; }
    dir="$ROOT/target/macos-libs"
    mkdir -p "$dir/lib"
    rm -f "$dir/lib/libz.tbd"
    perl -0pe 's/targets:(\s*)\[[^\]]*\]/targets:$1\[ arm64-macos, arm64e-macos, x86_64-macos \]/g' \
        "$sdk/usr/lib/libz.tbd" > "$dir/lib/libz.tbd"
    grep -q "install-name: *'/usr/lib/libz.1.dylib'" "$dir/lib/libz.tbd" \
        || { echo "ERROR: $sdk/usr/lib/libz.tbd is not /usr/lib/libz.1.dylib's stub" >&2; exit 1; }
    cat > "$dir/config.toml" <<EOF
# Written by scripts/desktop/macos-libs.sh: zlib from the macOS SDK.
[target.aarch64-apple-darwin.z]
rustc-link-lib = ["z"]
rustc-link-search = ["native=$dir/lib"]
EOF
    echo "$dir/config.toml"
    ;;
guard)
    bin="${2:?usage: macos-libs.sh guard <binary>}"
    name="$(basename "$bin")"
    for lib in $(otool -L "$bin" | awk 'NR>1 && $1 ~ /^\/nix\/store\// {print $1}'); do
        case "$lib" in
            */libiconv.2.dylib) install_name_tool -change "$lib" /usr/lib/libiconv.2.dylib "$bin" ;;
            */libz.1.dylib) install_name_tool -change "$lib" /usr/lib/libz.1.dylib "$bin" ;;
            *) ;;
        esac
    done
    if otool -L "$bin" | awk 'NR>1' | grep -q '/nix/store/'; then
        echo "ERROR: $name links libraries from /nix/store:" >&2
        otool -L "$bin" | grep '/nix/store/' >&2
        exit 1
    fi
    echo "$name links nothing from /nix/store:"
    otool -L "$bin" | awk 'NR>1 {print "  " $1}'
    ;;
*)
    echo "usage: $0 config | guard <binary>" >&2
    exit 2
    ;;
esac
