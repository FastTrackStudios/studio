#!/usr/bin/env bash
# Notarize a signed installer .pkg with Apple and staple the ticket to it,
# so Gatekeeper opens it offline with no warning. The same submission
# Studio's deploy-macos-pkg.sh makes (App Store Connect API key from
# ~/.appstoreconnect/config.env: ASC_KEY_PATH, ASC_KEY_ID, ASC_ISSUER_ID).
#
#   scripts/macos/notarize.sh dist/FTS-Plugins-<version>-macos.pkg
#   DRY_RUN=1 scripts/macos/notarize.sh …   # check the pkg and the key, print
#                                           # the commands, submit nothing
#
# Waits for Apple (minutes; occasionally much longer).
set -euo pipefail

# Apple's signing, packaging and notary tools, with Apple's own environment.
# Inside the devenv shell (where release.yml runs this) DEVELOPER_DIR and
# SDKROOT name the nix apple-sdk (14.4) and `xcrun` is nixpkgs' xcbuild shim,
# not Apple's: signing and notarizing are not to depend on either. Nothing
# here compiles (the bundles are built before this runs), so drop both and
# put /usr/bin first: codesign, security, pkgbuild, productbuild, pkgutil, xcrun, lipo.
unset DEVELOPER_DIR SDKROOT
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

pkg="${1:?usage: notarize.sh PKG}"
[ -f "$pkg" ] || { echo "notarize.sh: no file $pkg" >&2; exit 1; }

# Only a Developer-ID-Installer-signed pkg can be notarized.
pkgutil --check-signature "$pkg" | grep -q 'Developer ID Installer' \
    || { echo "notarize.sh: $pkg is not signed with a Developer ID Installer identity" >&2; exit 1; }

config="${ASC_CONFIG:-$HOME/.appstoreconnect/config.env}"
[ -f "$config" ] || { echo "notarize.sh: no App Store Connect key config at $config" >&2; exit 1; }
# shellcheck disable=SC1090
source "$config"
: "${ASC_KEY_PATH:?$config sets no ASC_KEY_PATH}" "${ASC_KEY_ID:?$config sets no ASC_KEY_ID}" "${ASC_ISSUER_ID:?$config sets no ASC_ISSUER_ID}"
[ -f "$ASC_KEY_PATH" ] || { echo "notarize.sh: the key file ASC_KEY_PATH names is missing" >&2; exit 1; }

if [ "${DRY_RUN:-}" = "1" ]; then
    echo "=== DRY_RUN=1: $pkg is Developer-ID-Installer signed and the API key is present; nothing submitted ==="
    echo "would run:"
    echo "  xcrun notarytool submit '$pkg' --key \"\$ASC_KEY_PATH\" --key-id \"\$ASC_KEY_ID\" --issuer \"\$ASC_ISSUER_ID\" --wait"
    echo "  xcrun stapler staple '$pkg'"
    echo "  xcrun stapler validate '$pkg'"
    exit 0
fi

echo "=== notarizing $pkg (this waits for Apple) ==="
out="$(xcrun notarytool submit "$pkg" \
    --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" \
    --wait 2>&1)" || { echo "$out" >&2; exit 1; }
echo "$out"
# `--wait` exits 0 for a finished submission even when Apple rejected it.
grep -q 'status: Accepted' <<<"$out" || {
    id="$(awk '/^  id:/{print $2; exit}' <<<"$out")"
    echo "notarize.sh: not accepted; the log: xcrun notarytool log $id --key … --key-id … --issuer …" >&2
    exit 1
}
echo "=== stapling ==="
xcrun stapler staple "$pkg"
xcrun stapler validate "$pkg"
spctl --assess --type install --verbose=2 "$pkg"
echo "=== DONE: notarized and stapled: $pkg ==="
