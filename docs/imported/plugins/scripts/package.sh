#!/usr/bin/env bash
# The Linux and Windows release archives: the named plugins' bundles, the
# install script for that platform and a short INSTALL.txt, in one archive
# (macOS has a signed .pkg instead: scripts/macos/pkg.sh).
#
#   scripts/package.sh eq comp …     (`just plugins-package` passes the suite)
#
# Writes dist/fts-plugins-<version>-<platform>.tar.gz (Linux) or .zip
# (Windows) and adds its line to dist/SHA256SUMS. Takes only the named
# bundles from target/bundled, never everything there.
#
# Env: VERSION=X.Y.Z (default: Cargo.toml's workspace version).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ $# -gt 0 ] || { echo "usage: package.sh <plugin> … (short names, e.g. eq comp)" >&2; exit 2; }

VERSION="${VERSION:-$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' Cargo.toml | head -1)}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "package.sh: version '$VERSION' is not X.Y.Z" >&2; exit 1; }

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) plat=linux-x86_64; kind=tar ;;
    Linux-aarch64) plat=linux-aarch64; kind=tar ;;
    MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) plat=windows-x86_64; kind=zip ;;
    Darwin-*) echo "package.sh: on macOS the release is the .pkg (just plugins-pkg)" >&2; exit 1 ;;
    *) echo "package.sh: unsupported platform $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

stem="fts-plugins-$VERSION-$plat"
stage="$ROOT/target/installer/$stem"
rm -rf "$stage"; mkdir -p "$stage"

n=0
while IFS= read -r name; do
    for ext in clap vst3; do
        src="target/bundled/$name.$ext"
        [ -e "$src" ] || { echo "package.sh: no $src (run 'just plugins-bundle')" >&2; exit 1; }
        cp -R "$src" "$stage/"
    done
    n=$((n+1))
done < <(bash scripts/bundles.sh "$@")

if [ "$kind" = tar ]; then
    cp scripts/install.sh "$stage/install.sh"; chmod +x "$stage/install.sh"
    cat > "$stage/INSTALL.txt" <<EOF
FTS Plugins $VERSION for Linux (x86-64-v3: AVX2, 2013 and later CPUs)

  ./install.sh            for you:        ~/.clap, ~/.vst3
  sudo ./install.sh --system   for everyone:  /usr/lib/clap, /usr/lib/vst3
  ./install.sh --dry-run  show what it would do

It replaces older copies of these plugins. Rescan plugins in your host.
Source and releases: https://github.com/FastTrackStudios/plugins
EOF
else
    cp scripts/install.ps1 "$stage/install.ps1"
    cat > "$stage/INSTALL.txt" <<EOF
FTS Plugins $VERSION for Windows (x64, AVX2: 2013 and later CPUs)

In PowerShell, in this folder:
  powershell -ExecutionPolicy Bypass -File .\install.ps1            for you
  powershell -ExecutionPolicy Bypass -File .\install.ps1 -System    for everyone (as administrator)

For you: %LOCALAPPDATA%\Programs\Common\CLAP and ...\VST3
For everyone: %COMMONPROGRAMFILES%\CLAP and %COMMONPROGRAMFILES%\VST3
It replaces older copies of these plugins. Rescan plugins in your host.
Source and releases: https://github.com/FastTrackStudios/plugins
EOF
fi
cp LICENSE "$stage/LICENSE.txt"

mkdir -p dist
if [ "$kind" = tar ]; then
    out="$stem.tar.gz"
    tar -czf "dist/$out" -C "$ROOT/target/installer" "$stem"
else
    out="$stem.zip"
    rm -f "dist/$out"
    if command -v 7z >/dev/null 2>&1; then
        (cd "$ROOT/target/installer" && 7z a -tzip -bso0 "$ROOT/dist/$out" "$stem")
    else
        (cd "$ROOT/target/installer" && zip -qr "$ROOT/dist/$out" "$stem")
    fi
fi
sum() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi; }
(cd dist && { grep -v "  $out\$" SHA256SUMS 2>/dev/null || true; sum "$out"; } > SHA256SUMS.new && mv SHA256SUMS.new SHA256SUMS)
echo "packaged $n plugins: dist/$out"
