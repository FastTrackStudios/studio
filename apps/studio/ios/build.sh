#!/usr/bin/env bash
# Studio for iPhone and iPad: the app drawn by Blitz, with the engine
# linked into it (iOS runs no second process; src/engine.rs).
#
#   apps/studio/ios/build.sh sim       a debug .app for the simulator
#   apps/studio/ios/build.sh device    a release .app for a phone (signed
#                                      and uploaded by deploy-testflight.sh)
#
# Prints the .app's path last. Enters the checkout's dev shell itself.
#
# The env dance: nixpkgs ships a fake xcbuild `xcrun`, and the dev shell's
# SDKROOT/DEVELOPER_DIR name the macOS SDK, so an iOS build needs the real
# xcrun first on PATH and both unset (the dev shell's CC_*_apple_ios* and
# CARGO_TARGET_*_LINKER wrappers do the rest, nix/modules/toolchain.nix).
#
# STUDIO_OTLP_ENDPOINT and STUDIO_OTLP_TOKEN, when set, are baked in
# (studio-otel's build.rs): where a phone, which starts with no
# environment, sends its spans and crash reports.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
if [ "${DEVENV_ROOT:-}" != "$root" ]; then
    exec bash "$root/scripts/dev-shell.sh" "${BASH_SOURCE[0]}" "$@"
fi

mode="${1:-sim}"
case "$mode" in
    sim) target=aarch64-apple-ios-sim; profile=debug; flags=() ;;
    device) target=aarch64-apple-ios; profile=release; flags=(--release) ;;
    *) echo "usage: $0 sim|device" >&2; exit 2 ;;
esac

bin_ios="${CARGO_TARGET_DIR:-$root/target}/bin-ios"
mkdir -p "$bin_ios"
ln -sf /usr/bin/xcrun "$bin_ios/xcrun"
export PATH="$bin_ios:$PATH"
unset DEVELOPER_DIR SDKROOT

cd "$root/apps/studio"
dx build --platform ios --renderer native --target "$target" \
    --no-default-features --features native "${flags[@]}"

# dx names the folder after the program (`Studio`, the package's [[bin]]).
out="${CARGO_TARGET_DIR:-$root/target}/dx/Studio/$profile/ios"
app="$(find "$out" -maxdepth 1 -name '*.app' -type d | head -1)"
[ -n "$app" ] || { echo "no .app under $out" >&2; exit 1; }

# What Studio needs of iOS beyond what dx writes.
plist="$app/Info.plist"
set_key() {
    /usr/libexec/PlistBuddy -c "Delete :$1" "$plist" >/dev/null 2>&1 || true
    /usr/libexec/PlistBuddy -c "Add :$1 $2 $3" "$plist"
}
set_key CFBundleDisplayName string Studio
# An app (dx omits it; the App Store requires it) and the oldest iOS it runs.
set_key CFBundlePackageType string APPL
set_key MinimumOSVersion string 16.0
# The system's launch screen (iPad multitasking requires one), dark like
# the app, so it opens without a white flash.
/usr/libexec/PlistBuddy -c "Add :UILaunchScreen dict" "$plist" >/dev/null 2>&1 || true
set_key UIUserInterfaceStyle string Dark
# The UIScene lifecycle: iOS 27 stops an app without it at launch. The
# delegate is the app's own (studio-shell's ios_scene, registered in main):
# winit, which Blitz draws into, does not adopt scenes itself yet.
/usr/libexec/PlistBuddy -c "Delete :UIApplicationSceneManifest" "$plist" >/dev/null 2>&1 || true
scene=":UIApplicationSceneManifest:UISceneConfigurations:UIWindowSceneSessionRoleApplication"
/usr/libexec/PlistBuddy \
    -c "Add :UIApplicationSceneManifest dict" \
    -c "Add :UIApplicationSceneManifest:UIApplicationSupportsMultipleScenes bool false" \
    -c "Add :UIApplicationSceneManifest:UISceneConfigurations dict" \
    -c "Add $scene array" \
    -c "Add $scene:0 dict" \
    -c "Add $scene:0:UISceneConfigurationName string Default" \
    -c "Add $scene:0:UISceneDelegateClassName string StudioSceneDelegate" \
    "$plist"
# The set plays on with the screen locked.
/usr/libexec/PlistBuddy -c "Delete :UIBackgroundModes" "$plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :UIBackgroundModes array" -c "Add :UIBackgroundModes:0 string audio" "$plist"
# cpal's audio session can ask for the microphone even when only output is
# used; Apple requires the reason to exist regardless.
set_key NSMicrophoneUsageDescription string "Studio records and monitors through your audio input when you ask it to."
# A room on the venue's Wi-Fi (the engine's peers on the local network).
set_key NSLocalNetworkUsageDescription string "Studio finds the others in your room on this network."
# The Bonjour type that raises the local-network prompt: without it iOS
# drops the peers' UDP silently instead of asking.
/usr/libexec/PlistBuddy -c "Delete :NSBonjourServices" "$plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :NSBonjourServices array" -c "Add :NSBonjourServices:0 string _fts._tcp" "$plist"
# A room's link in the app's own scheme (fasttrackstudio://join/<room>)
# opens Studio and joins the room (studio-shell's ios_scene::on_link).
/usr/libexec/PlistBuddy -c "Delete :CFBundleURLTypes" "$plist" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy \
    -c "Add :CFBundleURLTypes array" \
    -c "Add :CFBundleURLTypes:0 dict" \
    -c "Add :CFBundleURLTypes:0:CFBundleURLName string app.fasttrackstudio.studio" \
    -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes array" \
    -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes:0 string fasttrackstudio" \
    "$plist"
# No crypto beyond the system's: no export-compliance documents.
set_key ITSAppUsesNonExemptEncryption bool false
# Sessions and setlists in the Files app (Documents).
set_key UIFileSharingEnabled bool true
set_key LSSupportsOpeningDocumentsInPlace bool true
[ -z "${BUILD_NO:-}" ] || set_key CFBundleVersion string "$BUILD_NO"
[ -z "${MARKETING_VER:-}" ] || set_key CFBundleShortVersionString string "$MARKETING_VER"

# The simulator runs only signed code: signed again (ad hoc) after the
# edits. A device build is signed for distribution by deploy-testflight.sh.
if [ "$mode" = sim ]; then
    codesign --force --sign - --timestamp=none "$app" >/dev/null
fi
echo "$app"
