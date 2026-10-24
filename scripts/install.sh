#!/bin/sh
# Installs Studio (FastTrackStudio's Signal + Session app) from the newest
# GitHub Release. Needs only sh, curl or wget, and sha256sum or shasum.
#
#   curl -fsSL https://raw.githubusercontent.com/FastTrackStudios/studio/main/scripts/install.sh | sh
#
#   macOS (Apple silicon)  the notarized Studio-<version>-macos-arm64.dmg,
#                          copied into /Applications (or ~/Applications)
#   Linux (x86_64)         the Studio-<version>-linux-x86_64.AppImage into
#                          ~/.local/share/studio, `studio` on PATH, a desktop
#                          entry and icon
#
# The same release tag ships the iPhone/iPad app to TestFlight (README).
#
# Environment:
#   STUDIO_VERSION           exact version to install, e.g. 26.10.1
#                            (default: the newest release)
#   STUDIO_APP_DIR           macOS: where Studio.app goes (default
#                            /Applications, else ~/Applications)
#   STUDIO_HOME              Linux: where AppImages are kept (default
#                            ~/.local/share/studio)
#   STUDIO_BIN_DIR           Linux: where the `studio` link goes (default
#                            ~/.local/bin)
#   STUDIO_RELEASE_BASE_URL  mirror for releases/download (default: GitHub)
#
# Modelled on T3 Code's installer (https://t3.codes/install.sh): POSIX sh,
# no dependencies beyond a downloader and a checksum tool, every download
# checked against the release's SHA256SUMS before anything is installed.
set -eu

repo="FastTrackStudios/studio"
base_url="${STUDIO_RELEASE_BASE_URL:-https://github.com/${repo}/releases/download}"

fail() {
  printf '\nstudio install: %s\n' "$1" >&2
  exit 1
}
say() { printf '  %s\n' "$1" >&2; }

# ── Where we are ────────────────────────────────────────────────────────────
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin)
    case "$arch" in
      arm64) asset_suffix="macos-arm64.dmg" ;;
      *) fail "Studio for macOS is built for Apple silicon so far; this Mac is $arch" ;;
    esac ;;
  Linux)
    case "$arch" in
      x86_64 | amd64) asset_suffix="linux-x86_64.AppImage" ;;
      *) fail "Studio for Linux is built for x86_64 so far; this machine is $arch" ;;
    esac ;;
  MINGW* | MSYS* | CYGWIN*) fail "Studio has no Windows build yet" ;;
  *) fail "unsupported operating system $os" ;;
esac

# ── Tools ───────────────────────────────────────────────────────────────────
# fetch URL FILE: exit 44 on a 404, so "no such release" reads differently
# from a network failure.
fetch() {
  if command -v curl >/dev/null 2>&1; then
    status="$(curl -sSL -w '%{http_code}' "$1" -o "$2")" || return 1
    case "$status" in
      2??) return 0 ;;
      404) return 44 ;;
      *) printf 'GET %s returned HTTP %s\n' "$1" "$status" >&2; return 1 ;;
    esac
  elif command -v wget >/dev/null 2>&1; then
    wget -q --server-response "$1" -O "$2" 2>"$2.headers" && rm -f "$2.headers" && return 0
    if grep -q ' 404 ' "$2.headers" 2>/dev/null; then rm -f "$2.headers"; return 44; fi
    cat "$2.headers" >&2; rm -f "$2.headers"; return 1
  else
    fail "curl or wget is required"
  fi
}
if command -v sha256sum >/dev/null 2>&1; then
  checksum() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  checksum() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  fail "sha256sum or shasum is required"
fi

work="$(mktemp -d)"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ── Which release ───────────────────────────────────────────────────────────
version="${STUDIO_VERSION:-}"
version="${version#v}"
if [ -z "$version" ]; then
  say "Finding the newest release..."
  # /releases/latest is the newest release that is not a draft or a
  # prerelease. Its tag is vYY.M.MICRO (CalVer).
  fetch "https://api.github.com/repos/${repo}/releases/latest" "$work/latest.json" \
    || fail "could not read the newest release from GitHub"
  version="$(sed -n 's/.*"tag_name": *"v\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)".*/\1/p' "$work/latest.json" | head -n 1)"
  [ -n "$version" ] || fail "the newest release has no vX.Y.Z tag; set STUDIO_VERSION"
fi
case "$version" in
  *[!0-9.]* | '') fail "STUDIO_VERSION '$version' is not X.Y.Z" ;;
esac
asset="Studio-${version}-${asset_suffix}"
say "Installing Studio ${version} (${os} ${arch})"

# ── Download and verify ─────────────────────────────────────────────────────
# Each release carries one SHA256SUMS over its installers, attached after
# them, so a release that has it has the installers too.
rc=0
fetch "${base_url}/v${version}/SHA256SUMS" "$work/SHA256SUMS" || rc=$?
if [ "$rc" -eq 44 ]; then
  fail "release v${version} has no installers yet (the desktop builds may still be uploading; try again in a few minutes)"
elif [ "$rc" -ne 0 ]; then
  fail "could not download the release checksums"
fi
expected="$(grep " \*\{0,1\}${asset}\$" "$work/SHA256SUMS" | cut -d' ' -f1)"
[ -n "$expected" ] || fail "${asset} is not listed in release v${version}'s SHA256SUMS"
say "Downloading ${asset}..."
fetch "${base_url}/v${version}/${asset}" "$work/${asset}" || fail "could not download ${asset}"
actual="$(checksum "$work/${asset}")"
[ "$actual" = "$expected" ] || fail "checksum mismatch for ${asset}: not installing it"
say "Checksum verified."

# ── Install ─────────────────────────────────────────────────────────────────
install_macos() {
  app_dir="${STUDIO_APP_DIR:-}"
  if [ -z "$app_dir" ]; then
    if [ -w /Applications ]; then app_dir=/Applications; else app_dir="$HOME/Applications"; fi
  fi
  mkdir -p "$app_dir"
  mnt="$work/mnt"
  mkdir -p "$mnt"
  # diskutil image (macOS 26+; hdiutil attach is deprecated there), else
  # hdiutil on older systems.
  diskutil image attach --mountOptions nobrowse --readOnly --mountPoint "$mnt" "$work/${asset}" >/dev/null 2>&1 \
    || hdiutil attach "$work/${asset}" -nobrowse -readonly -mountpoint "$mnt" >/dev/null \
    || fail "could not open ${asset}"
  # Detach even if the copy fails.
  trap 'hdiutil detach "$mnt" >/dev/null 2>&1 || true; cleanup' EXIT
  [ -d "$mnt/Studio.app" ] || fail "${asset} holds no Studio.app"
  if [ -d "$app_dir/Studio.app" ]; then
    say "Replacing the Studio.app in $app_dir..."
    rm -rf "$app_dir/Studio.app"
  fi
  # ditto keeps the code signature and the stapled notarization ticket.
  ditto "$mnt/Studio.app" "$app_dir/Studio.app"
  hdiutil detach "$mnt" >/dev/null 2>&1 || true
  printf '\n  Installed Studio %s in %s\n  Open it from Launchpad, or: open "%s/Studio.app"\n\n' \
    "$version" "$app_dir" "$app_dir" >&2
}

install_linux() {
  home_dir="${STUDIO_HOME:-$HOME/.local/share/studio}"
  bin_dir="${STUDIO_BIN_DIR:-$HOME/.local/bin}"
  apps_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
  icons_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
  mkdir -p "$home_dir" "$bin_dir" "$apps_dir" "$icons_dir"
  dest="$home_dir/${asset}"
  mv "$work/${asset}" "$dest"
  chmod +x "$dest"
  ln -sfn "$dest" "$bin_dir/studio"
  # The icon the AppImage carries (the type2 runtime can extract a path).
  ( cd "$work" && "$dest" --appimage-extract .DirIcon >/dev/null 2>&1 ) || true
  if [ -f "$work/squashfs-root/.DirIcon" ]; then
    cp -L "$work/squashfs-root/.DirIcon" "$icons_dir/app.fasttrackstudio.studio.png"
  fi
  cat > "$apps_dir/app.fasttrackstudio.studio.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Studio
Comment=Signal and Session: live rigs and the DAW
Exec=$dest
Icon=app.fasttrackstudio.studio
Categories=AudioVideo;Audio;Music;
Terminal=false
DESKTOP
  # Older Studio AppImages are kept only until this one is in place.
  for old in "$home_dir"/Studio-*-linux-x86_64.AppImage; do
    [ "$old" = "$dest" ] || rm -f "$old"
  done
  printf '\n  Installed Studio %s\n' "$version" >&2
  case ":${PATH}:" in
    *":${bin_dir}:"*) printf '  Run studio, or open Studio from your app menu.\n\n' >&2 ;;
    *) printf '  Add %s to your PATH, then run studio (or open Studio from your app menu).\n\n' "$bin_dir" >&2 ;;
  esac
}

case "$os" in
  Darwin) install_macos ;;
  Linux) install_linux ;;
esac
