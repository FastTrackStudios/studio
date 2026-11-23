#!/usr/bin/env bash
# The macOS installer: one signed .pkg holding every FTS plugin, CLAP and
# VST3, universal (arm64 + x86_64).
#
# Ported from Studio's deploy-macos-pkg.sh, which shipped these plugins in
# FastTrackStudio-<ver>-macos.pkg until they moved here (studio#510). Same
# form, same component identifiers (app.fasttrackstudio.plugin.<slug>), so
# this package upgrades in place what Studio's installed:
#
#   [x] FTS Plugins          (group: toggles every plugin at once)
#       [x] FTS EQ           -> /Library/Audio/Plug-Ins/{CLAP,VST3}/FTS EQ.*
#       [x] FTS Comp
#       ... one per plugin, each installing its .clap and its .vst3
#
# Installer.app offers "all users" (admin) or "just me" (no password, into
# ~/Library/Audio/Plug-Ins). Scripted:
#   sudo installer -pkg FTS-Plugins-<ver>-macos.pkg -target /
#   installer -pkg FTS-Plugins-<ver>-macos.pkg -target CurrentUserHomeDirectory
#   sudo installer -applyChoiceChangesXML FTS-Plugins-<ver>-macos-choices.xml \
#        -pkg FTS-Plugins-<ver>-macos.pkg -target /       (pick plugins)
# No pre/post-install scripts, so an unattended install cannot hang.
#
#   scripts/macos/pkg.sh eq comp …     (`just plugins-pkg` passes the suite)
#
# Takes the bundles from target/bundled (`just plugins-bundle`), copies the
# named ones to target/installer, signs the copies (scripts/macos/sign.sh)
# and writes to dist/:
#   FTS-Plugins-<ver>-macos.pkg            signed "Developer ID Installer"
#   FTS-Plugins-<ver>-macos-choices.xml    every choice, for -applyChoiceChangesXML
#
# Env:
#   VERSION=X.Y.Z    the pkg's version (default: Cargo.toml's workspace version)
#   KEYCHAIN, KEYCHAIN_PW, ADHOC_SIGN   see sign.sh
#   PKG_UNSIGNED=1   no Installer signature (local structure test only;
#                    implies ADHOC_SIGN=1 for the bundles)
#   NOTARIZE=1       then notarize + staple (scripts/macos/notarize.sh); without
#                    it the pkg is signed but not notarized, and the command to
#                    notarize it is printed
set -euo pipefail

# Apple's signing, packaging and notary tools, with Apple's own environment.
# Inside the devenv shell (where release.yml runs this) DEVELOPER_DIR and
# SDKROOT name the nix apple-sdk (14.4) and `xcrun` is nixpkgs' xcbuild shim,
# not Apple's: signing and notarizing are not to depend on either. Nothing
# here compiles (the bundles are built before this runs), so drop both and
# put /usr/bin first: codesign, security, pkgbuild, productbuild, pkgutil, xcrun, lipo.
unset DEVELOPER_DIR SDKROOT
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
[ "$(uname)" = Darwin ] || { echo "pkg.sh: builds on macOS only" >&2; exit 1; }
[ $# -gt 0 ] || { echo "usage: pkg.sh <plugin> … (short names, e.g. eq comp)" >&2; exit 2; }

PRODUCT="FTS Plugins"
FILE_STEM="FTS-Plugins"
PKG_ID_PREFIX="app.fasttrackstudio"

VERSION="${VERSION:-$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' Cargo.toml | head -1)}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "pkg.sh: version '$VERSION' is not X.Y.Z" >&2; exit 1; }
echo "version: $VERSION"

BUNDLED="$ROOT/target/bundled"
STAGE="$ROOT/target/installer/bundles"
rm -rf "$STAGE"; mkdir -p "$STAGE"

# ── The named bundles only, copied out of target/bundled ─────────────────────
# Never "whatever is in target/bundled": a bundle built for something else
# (FTS Mic with the private models built in, say) must not ride along.
NAMES=()
while IFS= read -r n; do NAMES+=("$n"); done < <(bash scripts/bundles.sh "$@")
for name in "${NAMES[@]}"; do
    for ext in clap vst3; do
        src="$BUNDLED/$name.$ext"
        [ -d "$src" ] || { echo "pkg.sh: no $src (run 'just plugins-bundle')" >&2; exit 1; }
        # Universal or nothing: the pkg says it runs on both arches.
        bin="$(find "$src/Contents/MacOS" -type f -maxdepth 1 | head -1)"
        archs="$(lipo -archs "$bin" 2>/dev/null || true)"
        case " $archs " in *" arm64 "*) ;; *) echo "pkg.sh: $name.$ext has no arm64 slice ($archs)" >&2; exit 1 ;; esac
        case " $archs " in *" x86_64 "*) ;; *) echo "pkg.sh: $name.$ext has no x86_64 slice ($archs): build with 'just plugins-bundle' on macOS (bundle-universal)" >&2; exit 1 ;; esac
        cp -R "$src" "$STAGE/"
    done
done
echo "=== ${#NAMES[@]} plugins staged (CLAP + VST3, universal) ==="

# ── Sign the bundles ─────────────────────────────────────────────────────────
if [ "${PKG_UNSIGNED:-}" = "1" ]; then
    ADHOC_SIGN=1 bash scripts/macos/sign.sh "$STAGE"
else
    bash scripts/macos/sign.sh "$STAGE"
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PKGS="$WORK/pkgs"; mkdir -p "$PKGS"

# ── One component per plugin (its .clap and its .vst3 together) ─────────────
slug() { printf '%s' "$1" | sed -E 's/^FTS //; s/[^A-Za-z0-9]+/-/g' | tr '[:upper:]' '[:lower:]'; }
SLUGS=()
for name in "${NAMES[@]}"; do
    s="$(slug "$name")"
    root="$WORK/root-$s"
    mkdir -p "$root/Library/Audio/Plug-Ins/CLAP" "$root/Library/Audio/Plug-Ins/VST3"
    cp -R "$STAGE/$name.clap" "$root/Library/Audio/Plug-Ins/CLAP/"
    cp -R "$STAGE/$name.vst3" "$root/Library/Audio/Plug-Ins/VST3/"
    # --component-plist with BundleIsRelocatable=false: Installer must put a
    # plugin where the payload says, not "upgrade" a same-id bundle it finds
    # elsewhere on disk (a dev copy linked from a checkout, say).
    pkgbuild --analyze --root "$root" "$WORK/$s.plist" >/dev/null
    plutil -replace 0.BundleIsRelocatable -bool NO "$WORK/$s.plist" 2>/dev/null || true
    plutil -replace 1.BundleIsRelocatable -bool NO "$WORK/$s.plist" 2>/dev/null || true
    pkgbuild --quiet \
        --root "$root" \
        --component-plist "$WORK/$s.plist" \
        --install-location / \
        --identifier "$PKG_ID_PREFIX.plugin.$s" \
        --version "$VERSION" \
        "$PKGS/plugin-$s.pkg"
    SLUGS+=("$s")
    echo "  + $name ($PKG_ID_PREFIX.plugin.$s)"
done

# ── Distribution (the checkbox tree) ─────────────────────────────────────────
DIST="$WORK/distribution.xml"
{
    cat <<XML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>$PRODUCT $VERSION</title>
    <organization>$PKG_ID_PREFIX</organization>
    <domains enable_currentUserHome="true" enable_localSystem="true"/>
    <options customize="allow" require-scripts="false" hostArchitectures="arm64,x86_64"/>
    <welcome file="welcome.rtf" mime-type="text/rtf"/>
    <license file="license.txt" mime-type="text/plain"/>
    <conclusion file="conclusion.rtf" mime-type="text/rtf"/>
    <choices-outline>
        <line choice="choice.plugins">
XML
    for s in "${SLUGS[@]}"; do echo "            <line choice=\"choice.plugin.$s\"/>"; done
    cat <<XML
        </line>
    </choices-outline>
    <choice id="choice.plugins" title="$PRODUCT" start_selected="true"
            description="The FTS effects as CLAP and VST3 plugins. Untick to skip them all, or expand to choose."/>
XML
    for i in "${!SLUGS[@]}"; do
        s="${SLUGS[$i]}"; t="${NAMES[$i]}"
        cat <<XML
    <choice id="choice.plugin.$s" title="$t" start_selected="true" description="$t: CLAP and VST3.">
        <pkg-ref id="$PKG_ID_PREFIX.plugin.$s"/>
    </choice>
XML
    done
    for s in "${SLUGS[@]}"; do
        echo "    <pkg-ref id=\"$PKG_ID_PREFIX.plugin.$s\" version=\"$VERSION\">plugin-$s.pkg</pkg-ref>"
    done
    echo "</installer-gui-script>"
} > "$DIST"

mkdir -p dist
CHOICES="$ROOT/dist/$FILE_STEM-$VERSION-macos-choices.xml"
{
    cat <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- Silent install with these selections:
       sudo installer -applyChoiceChangesXML <this file> -pkg <the .pkg> -target /
     attributeSetting 1 = install, 0 = skip. Unlike the GUI, the group does
     not cascade: set each plugin. -->
<plist version="1.0">
<array>
XML
    for c in choice.plugins $(for s in "${SLUGS[@]}"; do echo "choice.plugin.$s"; done); do
        printf '  <dict>\n    <key>choiceIdentifier</key><string>%s</string>\n    <key>choiceAttribute</key><string>selected</string>\n    <key>attributeSetting</key><integer>1</integer>\n  </dict>\n' "$c"
    done
    echo "</array>"
    echo "</plist>"
} > "$CHOICES"

# ── productbuild (+ the Developer ID Installer signature) ────────────────────
RES="$WORK/resources"
cp -R scripts/macos/installer-resources "$RES"
cp LICENSE "$RES/license.txt"
PKG="$ROOT/dist/$FILE_STEM-$VERSION-macos.pkg"
rm -f "$PKG"

if [ "${PKG_UNSIGNED:-}" = "1" ]; then
    productbuild --distribution "$DIST" --package-path "$PKGS" --resources "$RES" "$PKG"
    echo "=== DONE: UNSIGNED pkg (testing only): $PKG ==="
    exit 0
fi

if [ -z "${KEYCHAIN:-}" ]; then
    if security list-keychains -d user | grep -q 'fts-build\.keychain'; then KEYCHAIN=fts-build.keychain; else KEYCHAIN=login.keychain-db; fi
fi
# "Developer ID Installer" is a different certificate from the Application
# one; `-p codesigning` would filter it out, so no policy here.
INSTALLER_ID="$(security find-identity -v "$KEYCHAIN" | awk -F'"' '/Developer ID Installer/{print $2; exit}')"
[ -n "$INSTALLER_ID" ] || { echo "pkg.sh: no \"Developer ID Installer\" identity in $KEYCHAIN (PKG_UNSIGNED=1 for a local test)" >&2; exit 1; }
echo "=== installer signed with: $INSTALLER_ID ==="
productbuild --distribution "$DIST" --package-path "$PKGS" --resources "$RES" \
    --keychain "$KEYCHAIN" --sign "$INSTALLER_ID" --timestamp "$PKG"
pkgutil --check-signature "$PKG"

if [ "${NOTARIZE:-}" = "1" ]; then
    bash scripts/macos/notarize.sh "$PKG"
else
    echo "=== signed, NOT notarized (NOTARIZE=1 to submit). To notarize this pkg: ==="
    echo "    bash scripts/macos/notarize.sh '$PKG'"
fi
echo "pkg:     $PKG"
echo "choices: $CHOICES"
