#!/usr/bin/env bash
# Developer-ID-sign every .clap/.vst3 bundle in a folder (hardened runtime,
# secure timestamp), inside out, and verify each signature.
#
# Ported from Studio's deploy-macos-plugins.sh (studio#510 moved the plugins
# here): the same keychain convention and codesign options.
#
#   scripts/macos/sign.sh DIR
#
# Identity: the first "Developer ID Application" in $KEYCHAIN (default
# fts-build.keychain when it is in the search list, else the login
# keychain). KEYCHAIN_PW, when set, unlocks it first (a headless runner).
# Without one it stops, unless ADHOC_SIGN=1: an ad-hoc signature for a local
# test build, which Gatekeeper will not accept on another Mac.
set -euo pipefail

# Apple's signing, packaging and notary tools, with Apple's own environment.
# Inside the devenv shell (where release.yml runs this) DEVELOPER_DIR and
# SDKROOT name the nix apple-sdk (14.4) and `xcrun` is nixpkgs' xcbuild shim,
# not Apple's: signing and notarizing are not to depend on either. Nothing
# here compiles (the bundles are built before this runs), so drop both and
# put /usr/bin first: codesign, security, pkgbuild, productbuild, pkgutil, xcrun, lipo.
unset DEVELOPER_DIR SDKROOT
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

dir="${1:?usage: sign.sh DIR}"
[ -d "$dir" ] || { echo "sign.sh: no folder $dir" >&2; exit 1; }

if [ "${ADHOC_SIGN:-}" = "1" ]; then
    SIGN_ID="-"
    OPTS=()
    echo "=== ADHOC_SIGN=1: ad-hoc signatures (local test only, not distributable) ==="
else
    if [ -z "${KEYCHAIN:-}" ]; then
        if security list-keychains -d user | grep -q 'fts-build\.keychain'; then
            KEYCHAIN=fts-build.keychain
        else
            KEYCHAIN=login.keychain-db
        fi
    fi
    [ -n "${KEYCHAIN_PW:-}" ] && security unlock-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
    SIGN_ID="$(security find-identity -v -p codesigning "$KEYCHAIN" \
        | awk -F'"' '/Developer ID Application/{print $2; exit}')"
    if [ -z "$SIGN_ID" ]; then
        cat >&2 <<MSG
sign.sh: no "Developer ID Application" identity in $KEYCHAIN.
  Release builds are signed on the machine that holds it (fleet convention:
  KEYCHAIN=fts-build.keychain). For a local test build: ADHOC_SIGN=1.
MSG
        exit 1
    fi
    OPTS=(--keychain "$KEYCHAIN" --options runtime --timestamp)
    echo "=== signing with: $SIGN_ID ==="
fi

n=0
for bundle in "$dir"/*.clap "$dir"/*.vst3; do
    [ -e "$bundle" ] || continue
    # Nested code first (none today, but a bundle that gains a dylib or a
    # framework must not ship it unsigned), then the bundle itself.
    if [ -d "$bundle" ]; then
        find "$bundle" \( -name "*.dylib" -o -name "*.so" -o -name "*.framework" \) -print0 \
            | while IFS= read -r -d '' f; do
                codesign --force ${OPTS[@]+"${OPTS[@]}"} --sign "$SIGN_ID" "$f"
              done
    fi
    codesign --force ${OPTS[@]+"${OPTS[@]}"} --sign "$SIGN_ID" "$bundle"
    codesign --verify --deep --strict "$bundle"
    printf '  signed  %s\n' "$(basename "$bundle")"
    n=$((n+1))
done
[ "$n" -gt 0 ] || { echo "sign.sh: no .clap/.vst3 bundles in $dir" >&2; exit 1; }
echo "=== $n bundle(s) signed and verified ==="
