#!/usr/bin/env bash
# Build one studio desktop app for Linux as an AppImage: the installer
# attached to the release's GitHub Release.
#
#   scripts/desktop/linux.sh signal      # -> target/desktop/Signal-<version>-linux-x86_64.AppImage
#   scripts/desktop/linux.sh session     # -> target/desktop/Session-<version>-linux-x86_64.AppImage
#
# Runs on THEBATTLESHIP (x86_64 NixOS runner), inside the repo's dev shell.
# Called by the `linux` job of .github/workflows/desktop.yml; a human can run
# it by hand.
#
# WHY AN APPIMAGE (the one format we ship): one file, no install step, runs
# on any recent distro without root or a package manager, and its --version
# can be smoke-tested without a display. A .deb would need dpkg and fits one
# family; a tarball has no desktop entry or icon. A binary built in a Nix
# shell is the catch: its interpreter and library paths point into
# /nix/store, which exists on no other machine. So this script
#   - copies every shared library the binary needs into the AppDir, except
#     the graphics stack (GL / EGL / Vulkan loader / DRM / GBM) which has to
#     come from the user's driver, and glibc itself, which comes from the
#     user's system;
#   - rewrites the interpreter to the standard /lib64/ld-linux-x86-64.so.2
#     and the RUNPATH to $ORIGIN, so nothing points at /nix/store;
#   - computes the newest glibc symbol version any bundled file asks for
#     (the AppImage's floor), prints it and refuses to build over
#     MAX_GLIBC, so a toolchain bump that raises the floor is noticed here.
#
# Env (all optional):
#   VERSION     X.Y.Z (default: [workspace.package])
#   OUT_DIR     default target/desktop
#   PROFILE     cargo profile (default release-fast)
#   MAX_GLIBC   highest glibc symbol version accepted (default 2.39, Ubuntu 24.04)
#   SKIP_BUILD  1 reuses $CARGO_TARGET_DIR (or target)/$PROFILE/<bin>
set -euo pipefail

app="${1:-}"
case "$app" in
    signal)
        crate=signal-desktop; bin=signal-desktop; title=Signal; features=""
        icon=apps/signal/desktop/ios/Assets.xcassets/AppIcon.appiconset/icon-1024.png
        comment="Guitar and keys rigs, sampler and tone engine"
        categories="AudioVideo;Audio;Music;"
        ;;
    session)
        crate=session-desktop; bin=session-desktop; title=Session; features=""
        icon=apps/session/desktop/ios/Assets.xcassets/AppIcon.appiconset/icon-1024.png
        comment="Setlists, songs, charts and the guide"
        categories="AudioVideo;Audio;Music;"
        ;;
    *) echo "usage: scripts/desktop/linux.sh <signal|session>" >&2; exit 2 ;;
esac

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
VERSION="${VERSION:-$(bash scripts/release.sh version)}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version '$VERSION' is not X.Y.Z" >&2; exit 2; }
OUT_DIR="${OUT_DIR:-$ROOT/target/desktop}"
PROFILE="${PROFILE:-release-fast}"
MAX_GLIBC="${MAX_GLIBC:-2.39}"
ARCH=x86_64
mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/$title-$VERSION-linux-$ARCH.AppImage"

# Re-run inside the CI dev shell (the one checks.yml builds the workspace in),
# then with the three packaging tools added from the same nixpkgs the flake is
# locked to (not whatever `nixpkgs` is today).
if [ -z "${DESKTOP_IN_SHELL:-}" ]; then
    exec nix develop --accept-flake-config "$ROOT#ci" \
        --command env DESKTOP_IN_SHELL=1 bash "$0" "$@"
fi
if [ -z "${DESKTOP_IN_TOOLS:-}" ]; then
    rev="$(python3 -c "import json;print(json.load(open('flake.lock'))['nodes']['nixpkgs']['locked']['rev'])" 2>/dev/null || true)"
    pk="nixpkgs"; [ -n "$rev" ] && pk="github:NixOS/nixpkgs/$rev"
    exec nix shell "$pk#patchelf" "$pk#appimagekit" "$pk#binutils" \
        --command env DESKTOP_IN_TOOLS=1 bash "$0" "$@"
fi

# ── Build ───────────────────────────────────────────────────────────────────
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/$PROFILE/$bin"
if [ "${SKIP_BUILD:-}" = 1 ]; then
    echo "=== SKIP_BUILD=1: reusing $BIN ==="
else
    echo "=== cargo build --profile $PROFILE -p $crate $features ==="
    # shellcheck disable=SC2086
    cargo build --profile "$PROFILE" -p "$crate" $features
fi
[ -x "$BIN" ] || { echo "ERROR: the build produced no $BIN" >&2; exit 1; }

# ── The AppDir ──────────────────────────────────────────────────────────────
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
AD="$WORK/$title.AppDir"
mkdir -p "$AD/usr/bin" "$AD/usr/lib" "$AD/usr/share/applications" "$AD/usr/share/icons/hicolor/256x256/apps"
cp "$BIN" "$AD/usr/bin/$bin"
chmod u+w "$AD/usr/bin/$bin"

# Not bundled: the user's graphics stack (driver-matched), and glibc itself.
excluded() {
    case "$1" in
        libGL.so*|libGLX*|libGLdispatch*|libOpenGL*|libEGL*|libGLESv*|libvulkan*|libdrm*|libgbm*) return 0 ;;
        libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libresolv.so*|libutil.so*|libnss_*|ld-linux*) return 0 ;;
        *) return 1 ;;
    esac
}

# Every library the binary needs, transitively (ldd already walks it).
declare -A seen=()
: > "$WORK/needed-excluded.txt"
while read -r name arrow path _; do
    [ "$arrow" = "=>" ] || continue
    [ -f "${path:-}" ] || continue
    if excluded "$name"; then echo "$name" >> "$WORK/needed-excluded.txt"; continue; fi
    [ -z "${seen[$name]:-}" ] || continue
    seen[$name]=1
    cp -L "$path" "$AD/usr/lib/$name"
    chmod u+w "$AD/usr/lib/$name"
done < <(ldd "$BIN")
echo "=== bundled ${#seen[@]} libraries; taken from the host at run time: $(sort -u "$WORK/needed-excluded.txt" | tr '\n' ' ')==="

# Nothing may point into /nix/store.
# shellcheck disable=SC2016 # $ORIGIN is for the dynamic loader, not the shell
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --set-rpath '$ORIGIN/../lib' "$AD/usr/bin/$bin"
for f in "$AD"/usr/lib/*; do
    # shellcheck disable=SC2016
    patchelf --set-rpath '$ORIGIN' "$f" 2>/dev/null || true
done
if grep -rl '/nix/store' "$AD/usr/bin" "$AD/usr/lib" >/dev/null 2>&1; then
    # Strings baked into a binary (build paths in panic messages) are fine;
    # ELF dynamic metadata is not.
    for f in "$AD"/usr/bin/* "$AD"/usr/lib/*; do
        if readelf -d "$f" 2>/dev/null | grep -E 'RPATH|RUNPATH|NEEDED' | grep -q '/nix/store'; then
            echo "ERROR: $f still names /nix/store in its dynamic section" >&2
            exit 1
        fi
        if readelf -l "$f" 2>/dev/null | grep -q 'interpreter: /nix/store'; then
            echo "ERROR: $f still has a /nix/store interpreter" >&2
            exit 1
        fi
    done
fi

# The floor: the newest GLIBC_x.y symbol version any bundled ELF requires.
floor="$(for f in "$AD"/usr/bin/* "$AD"/usr/lib/*; do
        objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' || true
    done | sed 's/GLIBC_//' | sort -uV | tail -1)"
echo "=== glibc floor of this AppImage: ${floor:-none} (limit $MAX_GLIBC) ==="
if [ -n "$floor" ] && [ "$(printf '%s\n%s\n' "$floor" "$MAX_GLIBC" | sort -V | tail -1)" != "$MAX_GLIBC" ]; then
    echo "ERROR: needs glibc $floor, over the limit $MAX_GLIBC: it would not start on the distros we claim." >&2
    echo "       Find the symbol: objdump -T <file> | grep GLIBC_$floor" >&2
    exit 1
fi
echo "$floor" > "$OUT_DIR/$title-glibc-floor.txt"

# ── Desktop entry, icon, AppRun ─────────────────────────────────────────────
slug="app.fasttrackstudio.$app"
cat > "$AD/$slug.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$title
Comment=$comment
Exec=$bin
Icon=$slug
Categories=$categories
Terminal=false
X-AppImage-Version=$VERSION
DESKTOP
cp "$AD/$slug.desktop" "$AD/usr/share/applications/$slug.desktop"
if command -v convert >/dev/null 2>&1; then
    convert "$icon" -resize 256x256 "$AD/$slug.png"
else
    cp "$icon" "$AD/$slug.png"
fi
cp "$AD/$slug.png" "$AD/usr/share/icons/hicolor/256x256/apps/$slug.png"
cp "$AD/$slug.png" "$AD/.DirIcon"

cat > "$AD/AppRun" <<APPRUN
#!/bin/sh
# The Vulkan loader finds a driver through XDG_DATA_DIRS' vulkan/icd.d; make
# sure the usual system directories are in it.
HERE="\$(dirname "\$(readlink -f "\$0")")"
export XDG_DATA_DIRS="\${XDG_DATA_DIRS:+\$XDG_DATA_DIRS:}/usr/local/share:/usr/share"
exec "\$HERE/usr/bin/$bin" "\$@"
APPRUN
chmod +x "$AD/AppRun"

# ── Pack ────────────────────────────────────────────────────────────────────
rm -f "$OUT"
ARCH=$ARCH VERSION="$VERSION" appimagetool --no-appstream "$AD" "$OUT"
chmod +x "$OUT"
( cd "$OUT_DIR" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256" )
echo "=== done: $OUT ($(du -h "$OUT" | cut -f1)) ==="
echo "appimage=$OUT"
