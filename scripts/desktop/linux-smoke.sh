#!/usr/bin/env bash
# Smoke-test a Linux AppImage headless: it must start, with no display and no
# audio, and print "<crate> <version>" for `--version`.
#
#   scripts/desktop/linux-smoke.sh <file.AppImage> <signal|session> <X.Y.Z>
#
# No display (DISPLAY and WAYLAND_DISPLAY are unset), no FUSE
# (APPIMAGE_EXTRACT_AND_RUN). On NixOS /lib64 has no real loader, which is
# exactly why the AppImage's interpreter is the standard path: there the
# check goes through `appimage-run` (an FHS sandbox with a stock glibc), and
# SMOKE_PODMAN_IMAGE=<image> (e.g. docker.io/library/ubuntu:24.04) runs it in
# a container of another distro, a stronger test of the same claim.
set -euo pipefail

img="${1:?usage: linux-smoke.sh <file.AppImage> <signal|session> <X.Y.Z>}"
app="${2:?app}"
want="${3:?version}"
case "$app" in signal) crate=signal-desktop ;; session) crate=session-desktop ;; *) echo "unknown app $app" >&2; exit 2 ;; esac
[ -x "$img" ] || { echo "$img is not an executable file" >&2; exit 1; }

env_clean=(env -u DISPLAY -u WAYLAND_DISPLAY APPIMAGE_EXTRACT_AND_RUN=1)
if [ -n "${SMOKE_PODMAN_IMAGE:-}" ]; then
    dir="$(cd "$(dirname "$img")" && pwd)"
    out="$(podman run --rm --network none -v "$dir:/a:ro" -e APPIMAGE_EXTRACT_AND_RUN=1 \
        "$SMOKE_PODMAN_IMAGE" "/a/$(basename "$img")" --version 2>&1)"
elif [ -e /lib64/ld-linux-x86-64.so.2 ] && /lib64/ld-linux-x86-64.so.2 --version >/dev/null 2>&1; then
    out="$("${env_clean[@]}" "$img" --version 2>&1)"
elif command -v appimage-run >/dev/null 2>&1; then
    out="$("${env_clean[@]}" appimage-run "$img" --version 2>&1)"
else
    echo "no way to run an FHS binary here (no /lib64 loader, no appimage-run, no SMOKE_PODMAN_IMAGE)" >&2
    exit 1
fi
echo "$out"
last="$(printf '%s\n' "$out" | tail -1)"
[ "$last" = "$crate $want" ] || { echo "ERROR: expected '$crate $want', got '$last'" >&2; exit 1; }
echo "smoke ok: $last"
