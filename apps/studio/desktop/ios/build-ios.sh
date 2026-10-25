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
# With --sim, also installs and relaunches on that simulator, microphone
# granted so its prompt does not cover the page. With no udid it uses an
# iPad Pro 11" in a device set ON THE BUILD DISK (SIM_SET, default
# /Volumes/build-disk/home/simulators), made and booted on first use:
# a simulator's apps and files never land on the Mac's small system drive
# (an install there filled it once). RELEASE=1 builds optimised (what TestFlight plays);
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
dx build --platform ios "${DX_PROFILE[@]}" --no-default-features --features signal-guitar,signal-keys-rig,signal-drums-rig,signal-ekit-rig,tone3000,session-app,doc-sync || true

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
    SIM="${2:-}"
    SET=()
    if [[ -z "$SIM" ]]; then
        SIM_SET="${SIM_SET:-/Volumes/build-disk/home/simulators}"
        mkdir -p "$SIM_SET"
        SET=(--set "$SIM_SET")
        NAME="iPad Pro 11 (build disk)"
        SIM="$(xcrun simctl "${SET[@]}" list devices available | grep -F "$NAME" | grep -oE '[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}' | head -1 || true)"
        if [[ -z "$SIM" ]]; then
            RUNTIME="$(xcrun simctl list runtimes available | grep -oE 'com\.apple\.CoreSimulator\.SimRuntime\.iOS-[0-9-]+' | tail -1)"
            TYPE="$(xcrun simctl list devicetypes | grep -oE 'com\.apple\.CoreSimulator\.SimDeviceType\.iPad-Pro-11-inch-M[0-9]+[A-Za-z0-9-]*' | tail -1)"
            if ! SIM="$(xcrun simctl "${SET[@]}" create "$NAME" "$TYPE" "$RUNTIME")"; then
                # Never fall back to the system drive. The usual cause:
                # CoreSimulatorService may not write to an external volume.
                echo "ERROR: no simulator could be made in $SIM_SET." >&2
                echo "  Allow it: System Settings > Privacy & Security > Full Disk Access," >&2
                echo "  add /Library/Developer/PrivateFrameworks/CoreSimulator.framework/" >&2
                echo "  Versions/A/XPCServices/com.apple.CoreSimulator.CoreSimulatorService.xpc," >&2
                echo "  then run again. Or name a simulator: just ios-sim '' <udid>." >&2
                exit 1
            fi
            echo "made simulator $SIM in $SIM_SET"
        fi
        xcrun simctl "${SET[@]}" boot "$SIM" 2>/dev/null || true
    fi
    ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
    xcrun simctl "${SET[@]}" install "$SIM" "$APP"
    xcrun simctl "${SET[@]}" privacy "$SIM" grant microphone "$ID" 2>/dev/null || true
    if [[ -n "${SIM_GIGS:-}" ]]; then
        DOCS="$(xcrun simctl "${SET[@]}" get_app_container "$SIM" "$ID" data)/Documents/FastTrackStudio/Gigs"
        mkdir -p "$DOCS"
        for g in $SIM_GIGS; do
            rsync -a --exclude='*.bak' --exclude='.DS_Store' "$g" "$DOCS/"
            echo "gig in Documents: $(basename "$g")"
        done
    fi
    xcrun simctl "${SET[@]}" launch --terminate-running-process "$SIM" "$ID"
    echo "launched on simulator $SIM"
fi
