#!/usr/bin/env bash
# Studio for iPhone and iPad to TestFlight: the release .app (build.sh
# device: the app with the engine linked in), signed for the App Store and
# uploaded through the App Store Connect API. The distribution certificate
# and the profile are minted through the API on first use, so no Xcode UI
# is needed. The App Store Connect key lives in ~/.appstoreconnect.
#
#   Interactive Mac (login keychain, a GUI session):
#     MARKETING_VER=26.12.0 apps/studio/ios/deploy-testflight.sh
#   Headless build box (airlock): run setup-keychain.sh once, then
#     KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build MARKETING_VER=26.12.0 \
#       apps/studio/ios/deploy-testflight.sh
#   (a keychain of its own lets codesign work over SSH with nobody logged in).
#
# SKIP_BUILD=1 signs and uploads the .app already built (iterating on the
# signing). STUDIO_OTLP_ENDPOINT and STUDIO_OTLP_TOKEN, when set, are baked
# into the build (studio-otel): where the phones send their spans and crash
# reports.
set -euo pipefail

# The marketing version (CFBundleShortVersionString) is the release tag's
# (studio-v26.12.0 -> 26.12.0). Checked first: a missing version must not
# cost a whole build before it fails.
: "${MARKETING_VER:?MARKETING_VER is required: the version of the release tag being shipped (e.g. 26.12.0)}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
# What the desktop's installers share with this one: the build keychain's
# setup and the app's icon.
SHARED="$ROOT/crates/studio-engine/engine/engine-host/ios"

TMP_DIRS=()
cleanup_tmp() { [ ${#TMP_DIRS[@]} -eq 0 ] || rm -rf "${TMP_DIRS[@]}"; }
trap cleanup_tmp EXIT

# Each upload needs a higher build number than the last: the unix time.
BUILD_NO="${BUILD_NO:-$(date +%s)}"
export BUILD_NO MARKETING_VER

# shellcheck disable=SC1090,SC1091
source "$HOME/.appstoreconnect/config.env"

# Signing keychain. On a headless box set KEYCHAIN + KEYCHAIN_PW to the one
# setup-keychain.sh made: over SSH the login keychain cannot sign
# (errSecInternalComponent).
KEYCHAIN="${KEYCHAIN:-login.keychain-db}"
KEYCHAIN_PW="${KEYCHAIN_PW:-}"
if [ -n "$KEYCHAIN_PW" ]; then
    security unlock-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
    security list-keychains -d user -s "$KEYCHAIN" login.keychain-db >/dev/null 2>&1 || true
fi

# The distribution identity, made through the API and imported when the
# keychain has none yet.
if ! security find-identity -v -p codesigning "$KEYCHAIN" | grep -q "Apple Distribution"; then
    echo "=== creating Apple Distribution certificate ==="
    eval "$(ruby "$SCRIPT_DIR/mint-dist-cert.rb" | grep -E '^DIST_(KEY|CER)=')"
    work="$(mktemp -d)"
    TMP_DIRS+=("$work")
    openssl x509 -inform DER -in "$DIST_CER" -out "$work/dist.pem"
    if openssl pkcs12 -help 2>&1 | grep -q -- -legacy; then LEG=(-legacy); else LEG=(); fi
    openssl pkcs12 -export "${LEG[@]}" -inkey "$DIST_KEY" -in "$work/dist.pem" \
        -name "Apple Distribution" -out "$work/dist.p12" -passout pass:fts
    security import "$work/dist.p12" -k "$KEYCHAIN" -P fts -A -T /usr/bin/codesign
fi
SIGN_ID="$(security find-identity -v -p codesigning "$KEYCHAIN" \
    | awk -F'"' '/Apple Distribution/{print $2; exit}')"
[ -n "$SIGN_ID" ] || { echo "ERROR: no distribution identity after the import" >&2; exit 1; }
echo "=== distribution identity: $SIGN_ID (keychain: $KEYCHAIN) ==="

echo "=== building release ==="
APP_GLOB="${CARGO_TARGET_DIR:-$ROOT/target}/dx/Studio/release/ios"
if [ "${SKIP_BUILD:-}" = "1" ] && [ -n "$(find "$APP_GLOB" -maxdepth 1 -name '*.app' 2>/dev/null)" ]; then
    APP="$(find "$APP_GLOB" -maxdepth 1 -name '*.app' -type d | head -1)"
    echo "SKIP_BUILD=1: reusing $APP"
else
    APP="$(bash "$SCRIPT_DIR/build.sh" device | tail -1)"
fi
[ -d "$APP" ] || { echo "ERROR: the release build made no .app" >&2; exit 1; }
BUNDLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
PLIST="$APP/Info.plist"

echo "=== App Store provisioning profile ($BUNDLE) ==="
PROFILE_NAME="${PROFILE_NAME:-FTS ${BUNDLE##*.} App Store}"
PROFILE="$(PROFILE_TYPE=IOS_APP_STORE CERT_TYPE=DISTRIBUTION \
    ruby "$SCRIPT_DIR/mint-dev-profile.rb" - "$BUNDLE" "$PROFILE_NAME" | awk -F= '/PROFILE_PATH=/{print $2}')"
echo "profile: $PROFILE"

# One platform: dx leaves both iPhoneOS and iPadOS, which Apple refuses
# (91177). An iPhone app runs on an iPad.
/usr/libexec/PlistBuddy -c "Delete :CFBundleSupportedPlatforms" "$PLIST" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms array" -c "Add :CFBundleSupportedPlatforms:0 string iPhoneOS" "$PLIST"

# The SDK's build metadata. Xcode's build writes it and dx does not; without
# it App Store Connect cannot tell the SDK and refuses the upload (90534).
DEV="${XCODE_DIR:-$(xcode-select -p)}"
SDK="$DEV/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk"
SDK_VER="$(/usr/libexec/PlistBuddy -c 'Print :Version' "$SDK/SDKSettings.plist" 2>/dev/null || true)"
SDK_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :ProductBuildVersion' "$DEV/Platforms/iPhoneOS.platform/version.plist" 2>/dev/null || true)"
XCODE_ROOT="${DEV%/Contents/Developer}"
XCODE_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :ProductBuildVersion' "$XCODE_ROOT/Contents/version.plist" 2>/dev/null || true)"
XCODE_VER="$(DEVELOPER_DIR="$DEV" xcodebuild -version 2>/dev/null | awk '/^Xcode/{print $2}' || true)"
# DTXcode = major*100 + minor*10 + patch, four digits (26.6 -> 2660).
DTXCODE="$(echo "$XCODE_VER" | awk -F. '{printf "%02d%d%d", $1, ($2==""?0:$2), ($3==""?0:$3)}')"
add_str() {
    /usr/libexec/PlistBuddy -c "Add :$1 string $2" "$PLIST" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Set :$1 $2" "$PLIST" 2>/dev/null || true
}
add_str DTPlatformName iphoneos
add_str DTPlatformVersion "$SDK_VER"
add_str DTPlatformBuild "$SDK_BUILD"
add_str DTSDKName "iphoneos${SDK_VER}"
add_str DTSDKBuild "$SDK_BUILD"
add_str DTXcode "$DTXCODE"
add_str DTXcodeBuild "$XCODE_BUILD"
add_str DTCompiler "com.apple.compilers.llvm.clang.1_0"
add_str BuildMachineOSBuild "$(sw_vers -buildVersion)"
echo "=== SDK metadata: iphoneos${SDK_VER} (${SDK_BUILD}), Xcode ${XCODE_VER} (${XCODE_BUILD}) ==="

# The home-screen icon (dx makes none): actool compiles the asset catalog
# into Assets.car and writes the icon keys to a partial plist, merged in.
# ACTOOL_DEVELOPER_DIR may name another Xcode for this step only, when this
# one's actool does not match the running macOS.
ICONS_DIR="${ICONS_DIR:-$SHARED/Assets.xcassets}"
ICON_DEV="${ACTOOL_DEVELOPER_DIR:-$DEV}"
ACTOOL="$ICON_DEV/usr/bin/actool"; [ -x "$ACTOOL" ] || ACTOOL="$(xcrun --find actool)"
work="$(mktemp -d)"
TMP_DIRS+=("$work")
DEVELOPER_DIR="$ICON_DEV" "$ACTOOL" "$ICONS_DIR" --compile "$APP" --platform iphoneos \
    --minimum-deployment-target 16.0 --app-icon AppIcon \
    --output-partial-info-plist "$work/icon.plist" >"$work/actool.log" 2>&1 || true
if [ -f "$APP/Assets.car" ] && /usr/libexec/PlistBuddy -c "Print :CFBundleIcons" "$work/icon.plist" >/dev/null 2>&1; then
    /usr/libexec/PlistBuddy -c "Merge $work/icon.plist" "$PLIST"
    add_str CFBundleIconName AppIcon
    echo "app icon embedded"
else
    echo "ERROR: the icon compiled to no Assets.car (the upload would be refused); set ACTOOL_DEVELOPER_DIR" >&2
    tail -6 "$work/actool.log" >&2
    exit 1
fi

echo "=== signing (distribution) ==="
cp "$PROFILE" "$APP/embedded.mobileprovision"
security cms -D -i "$PROFILE" > "$work/profile.plist"
/usr/libexec/PlistBuddy -x -c "Print :Entitlements" "$work/profile.plist" > "$work/entitlements.plist"
find "$APP" \( -name "*.dylib" -o -name "*.framework" \) -print0 \
    | while IFS= read -r -d '' f; do codesign --force --keychain "$KEYCHAIN" --sign "$SIGN_ID" --timestamp "$f"; done
codesign --force --keychain "$KEYCHAIN" --sign "$SIGN_ID" --entitlements "$work/entitlements.plist" --timestamp "$APP"
codesign --verify --deep --strict "$APP"

echo "=== packaging the IPA ==="
mkdir -p "$work/Payload"
cp -R "$APP" "$work/Payload/"
IPA="$work/Studio.ipa"
# ditto, not zip: it keeps the _CodeSignature/CodeResources link altool needs.
( cd "$work" && ditto -c -k --sequesterRsrc --keepParent Payload "$IPA" )

echo "=== uploading to TestFlight ==="
xcrun altool --upload-app -t ios -f "$IPA" --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
echo "build $BUILD_NO"
echo "=== DONE: build $BUILD_NO ($BUNDLE $MARKETING_VER) uploaded; TestFlight shows it once Apple has processed it (5-15 min) ==="
