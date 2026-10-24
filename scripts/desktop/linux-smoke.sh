#!/usr/bin/env bash
# Smoke-test a Linux AppImage headless: it must start, with no display and no
# audio, and print "studio-desktop <version>" for `--version`.
#
#   scripts/desktop/linux-smoke.sh <file.AppImage> <X.Y.Z>
#
# No display (DISPLAY and WAYLAND_DISPLAY are unset), no FUSE
# (APPIMAGE_EXTRACT_AND_RUN). On NixOS /lib64 has no real loader, which is
# exactly why the AppImage's interpreter is the standard path: there the
# check goes through `appimage-run` (an FHS sandbox with a stock glibc), and
# SMOKE_PODMAN_IMAGE=<image> (e.g. docker.io/library/archlinux:latest) runs its
# payload in a container of another distro, a stronger test of the same claim.
set -euo pipefail
# Say where it stopped: a runner's hardening fails these in ways that print
# nothing of their own.
trap 'echo "linux-smoke: failed at line $LINENO: $BASH_COMMAND" >&2' ERR

img="${1:?usage: linux-smoke.sh <file.AppImage> <X.Y.Z>}"
want="${2:?version}"
crate=studio-desktop
[ -x "$img" ] || { echo "$img is not an executable file" >&2; exit 1; }
img="$(cd "$(dirname "$img")" && pwd)/$(basename "$img")"

env_clean=(env -u DISPLAY -u WAYLAND_DISPLAY APPIMAGE_EXTRACT_AND_RUN=1)
if [ -n "${SMOKE_PODMAN_IMAGE:-}" ]; then
    # The payload (the squashfs after the pinned runtime) is extracted and
    # its AppRun run in the container: a host that registers a binfmt handler
    # for AppImages (NixOS's appimage-run does) would hijack the file inside
    # the container too, to an interpreter the image does not have.
    off="${SMOKE_RUNTIME_SIZE:-944632}"
    work="$(mktemp -d -p "$(dirname "$img")" .smoke.XXXXXX)"
    trap 'rm -rf "$work"' EXIT
    if command -v unsquashfs >/dev/null 2>&1; then
        unsquashfs -q -o "$off" -d "$work/app" "$img" >/dev/null
    else
        nix shell nixpkgs#squashfsTools -c unsquashfs -q -o "$off" -d "$work/app" "$img" >/dev/null
    fi
    out="$(podman run --rm --network none -v "$work/app:/app:ro" "$SMOKE_PODMAN_IMAGE" /app/AppRun --version 2>&1)" || { echo "$out"; exit 1; }
else
  # Each way in turn; the first that runs it wins. A host's /lib64 loader or
  # appimage-run can both end in bubblewrap (NixOS registers a binfmt handler
  # for AppImages), which needs user namespaces a hardened runner service
  # may not have: then the payload below. SMOKE_FORCE_PAYLOAD=1 goes straight
  # there (how that path is tested on a host that has namespaces).
  ran=0; out=""
  if [ "${SMOKE_FORCE_PAYLOAD:-0}" != 1 ]; then
    if [ -e /lib64/ld-linux-x86-64.so.2 ] && /lib64/ld-linux-x86-64.so.2 --version >/dev/null 2>&1; then
      echo "trying: the file itself (the host has an FHS loader)" >&2
      if out="$("${env_clean[@]}" "$img" --version 2>&1)"; then ran=1; else echo "  it did not start: $(printf '%s\n' "$out" | tail -1)" >&2; fi
    fi
    if [ "$ran" = 0 ] && command -v appimage-run >/dev/null 2>&1; then
      echo "trying: appimage-run" >&2
      if out="$("${env_clean[@]}" appimage-run "$img" --version 2>&1)"; then ran=1; else echo "  it did not start: $(printf '%s\n' "$out" | tail -1)" >&2; fi
    fi
  fi
  if [ "$ran" = 0 ]; then
    # No FHS loader, and no sandbox: appimage-run's bubblewrap needs user
    # namespaces, which a hardened runner service may not have ("bwrap: No
    # permissions to create a new namespace", THEBATTLESHIP, 2026-10-24).
    # Then: the payload, unpacked, run through THIS host's glibc loader with
    # only the bundled libraries on the path. Still the claim that matters:
    # everything the binary needs but glibc and the graphics stack is in the
    # bundle, and it starts without a display.
    echo "trying: the payload through the host loader" >&2
    # Where the squashfs starts: the end of the runtime ELF's section
    # header table (e_shoff + e_shentsize * e_shnum), which is how the type2
    # runtime finds it (--appimage-offset). Read, not run: running the file
    # is what bubblewrap refused.
    shoff="$(od -An -t u8 -j 40 -N 8 "$img" | tr -d ' ')"
    shentsize="$(od -An -t u2 -j 58 -N 2 "$img" | tr -d ' ')"
    shnum="$(od -An -t u2 -j 60 -N 2 "$img" | tr -d ' ')"
    off=$((shoff + shentsize * shnum))
    work="$(mktemp -d -p "$(dirname "$img")" .smoke.XXXXXX)"
    trap 'rm -rf "$work"' EXIT
    if command -v unsquashfs >/dev/null 2>&1; then
        unsquashfs -q -o "$off" -d "$work/app" "$img" >/dev/null
    else
        nix shell nixpkgs#squashfsTools -c unsquashfs -q -o "$off" -d "$work/app" "$img" >/dev/null
    fi
    ld="$(ldd "$(command -v bash)" 2>/dev/null | awk '/ld-linux-x86-64/ { print ($3 != "" ? $3 : $1) }' | head -1)"
    [ -x "$ld" ] || { echo "no way to run the payload here (no /lib64 loader, no usable appimage-run, no host glibc loader)" >&2; exit 1; }
    echo "running the payload through the host loader $ld" >&2
    out="$(env -u DISPLAY -u WAYLAND_DISPLAY "$ld" --library-path "$work/app/usr/lib" "$work/app/usr/bin/$crate" --version 2>&1)" \
        || { echo "$out"; exit 1; }
  fi
fi
echo "$out"
last="$(printf '%s\n' "$out" | tail -1)"
[ "$last" = "$crate $want" ] || { echo "ERROR: expected '$crate $want', got '$last'" >&2; exit 1; }
echo "smoke ok: $last"
