#!/usr/bin/env bash
# Build the Studio iPhone/iPad app (Signal's in-process rigs + Session).
#
# Run on a Mac inside the repo's nix dev shell. The env dance is REQUIRED:
# nixpkgs ships a fake xcbuild `xcrun` and its SDK env breaks Xcode's, so
# iOS cross-compiles need the real xcrun first on PATH and the nix SDK vars
# unset (the flake's CARGO_TARGET_*_LINKER / CC_* handle the rest).
#
#   cd apps/studio/desktop && ./ios/build-ios.sh [--sim [<udid>]]
#   just ios-sim                      # the same, release, on the booted sim
#
# With --sim, also installs and relaunches on that simulator (the booted
# one when no udid is given), microphone granted so its prompt does not
# cover the page. RELEASE=1 builds optimised (what TestFlight plays);
# SIM_GIGS="<gig folder> …" copies gigs into the app's Documents there,
# as Files does on an iPad.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE/.."

BIN_IOS="$HOME/bin-ios"
mkdir -p "$BIN_IOS"
ln -sf /usr/bin/xcrun "$BIN_IOS/xcrun"
ln -sf /usr/bin/xcodebuild "$BIN_IOS/xcodebuild"

unset DEVELOPER_DIR SDKROOT
export PATH="$BIN_IOS:$PATH"
# The floor the devshell sets (nix/modules/toolchain.nix) and Info.plist's
# MinimumOSVersion: a build outside nix otherwise links for iOS 10 while
# the C++ objects (NAM's Eigen) were compiled for the SDK's own version, and
# `___chkstk_darwin` is missing at the link.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"

# dx can exit non-zero after a good build (its own codesign step): gate on
# the .app below instead.
PROFILE=debug
DX_PROFILE=()
if [[ "${RELEASE:-0}" == "1" ]]; then
    PROFILE=release
    DX_PROFILE=(--release)
fi
dx build --platform ios "${DX_PROFILE[@]}" --no-default-features --features signal-guitar,signal-keys-rig,signal-drums-rig,signal-ekit-rig,tone3000,session-app || true

# dx names the bundle from the package (StudioDesktop.app today; it has
# changed between dx versions), so take the newest one it wrote.
# shellcheck disable=SC2012 # dx's own bundle names: no spaces to mind
APP="$(ls -dt "$(cd "$HERE/../../../.." && pwd)"/target/dx/studio-desktop/"$PROFILE"/ios/*.app 2>/dev/null | head -1)"
[ -n "$APP" ] && [ -f "$APP/Info.plist" ] || { echo "ERROR: dx produced no app" >&2; exit 1; }

# Info.plist (scene manifest, dark, usage strings) and frame's faces —
# the same as the TestFlight build writes.
# shellcheck source=app-plist.sh disable=SC1091
source "$HERE/app-plist.sh"
studio_app_plist "$APP"
studio_app_faces "$APP"
# The gigs it carries (GIG_BUNDLE, as deploy-testflight.sh).
# shellcheck disable=SC2086 # one argument per gig folder
studio_app_gigs "$APP" ${GIG_BUNDLE:-}

# IPAD_LANDSCAPE=1 (development only — TestFlight keeps every orientation
# and multitasking): the iPad held sideways, full screen. A simulator here
# has no Simulator app to turn it, and iPadOS's windowing mode refuses a
# programmatic turn.
if [[ "${IPAD_LANDSCAPE:-0}" == "1" ]]; then
    PB=/usr/libexec/PlistBuddy
    $PB -c "Delete :UISupportedInterfaceOrientations~ipad" "$APP/Info.plist" 2>/dev/null || true
    $PB -c "Add :UISupportedInterfaceOrientations~ipad array" "$APP/Info.plist"
    $PB -c "Add :UISupportedInterfaceOrientations~ipad:0 string UIInterfaceOrientationLandscapeRight" "$APP/Info.plist"
    $PB -c "Add :UISupportedInterfaceOrientations~ipad:1 string UIInterfaceOrientationLandscapeLeft" "$APP/Info.plist"
    $PB -c "Delete :UIRequiresFullScreen" "$APP/Info.plist" 2>/dev/null || true
    $PB -c "Add :UIRequiresFullScreen bool true" "$APP/Info.plist"
    echo "iPad: landscape only, full screen (development)"
fi

# The plist edits above broke dx's signature: sign it again, ad hoc (a
# simulator wants a valid signature, not a team's).
codesign --force --sign - --timestamp=none "$APP" >/dev/null

echo "built: $APP"

if [[ "${1:-}" == "--sim" ]]; then
    SIM="${2:-booted}"
    ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
    xcrun simctl install "$SIM" "$APP"
    xcrun simctl privacy "$SIM" grant microphone "$ID" 2>/dev/null || true
    if [[ -n "${SIM_GIGS:-}" ]]; then
        DOCS="$(xcrun simctl get_app_container "$SIM" "$ID" data)/Documents/FastTrackStudio/Gigs"
        mkdir -p "$DOCS"
        for g in $SIM_GIGS; do
            rsync -a --exclude='*.bak' --exclude='.DS_Store' "$g" "$DOCS/"
            echo "gig in Documents: $(basename "$g")"
        done
    fi
    xcrun simctl launch --terminate-running-process "$SIM" "$ID"
    echo "launched on simulator $SIM"
fi
