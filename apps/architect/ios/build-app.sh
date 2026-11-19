#!/usr/bin/env bash
# Studio's distribution of Architect as an iOS app bundle (docs/studio-ios.md):
# `studio-architect` built for the Simulator or a device and put into
# `<target>/<triple>/<profile>/ArchitectMobile.app` by Architect's own
# bundler (`apps/mobile/ios/build-app.sh` of the Architect checkout cargo
# resolves), with this folder's Info.plist (bundle id
# `app.fasttrackstudio.studio-architect`, name "Studio") and Studio's
# icon. It prints the bundle's path.
#
#   apps/architect/ios/build-app.sh [--device] [--release] [cargo flags…]
#   apps/architect/ios/build-app.sh --config ../stack/overrides.toml        # the side-by-side stack
#
# The Simulator's bundle is signed ad hoc; a device's is left unsigned.
# This is not the shipping app (`apps/studio/desktop`, bundle
# `app.fasttrackstudio.studio`): it has no signing, upload or CI lane.
#
# ARCHITECT_SRC: the Architect checkout to take the bundler from (else the
# one `architect-mobile` resolves to, with the same cargo flags).
# ACTOOL_DEVELOPER_DIR: an Xcode whose actool runs on this macOS.
set -euo pipefail

die() { echo "studio ios: $*" >&2; exit 1; }

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"

# cargo's `--config` flags, for the metadata call that finds Architect.
configs=()
want=""
for a in "$@"; do
    if [[ -n "$want" ]]; then configs+=(--config "$a"); want=""; continue; fi
    case "$a" in
        --config) want=1 ;;
        --config=*) configs+=("$a") ;;
    esac
done

src="${ARCHITECT_SRC:-}"
if [[ -z "$src" ]]; then
    manifest="$(cd "$root" && cargo ${configs[@]+"${configs[@]}"} metadata --format-version 1 \
        --filter-platform aarch64-apple-ios 2>/dev/null | python3 -I -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "architect-mobile":
        print(p["manifest_path"])
        break
')"
    [[ -n "$manifest" ]] || die "cargo resolves no architect-mobile"
    src="$(cd "$(dirname "$manifest")/../.." && pwd)"
fi
bundler="$src/apps/mobile/ios/build-app.sh"
[[ -f "$bundler" ]] || die "no $bundler"

# The catalog: this folder's launch colour and the Studio icon the
# shipping app has (one icon for Studio, kept once).
target="${CARGO_TARGET_DIR:-$root/target}"
assets="$target/studio-ios-assets/Assets.xcassets"
rm -rf "$assets"
mkdir -p "$assets"
cp -R "$here/Assets.xcassets/." "$assets/"
cp -R "$root/apps/studio/desktop/ios/Assets.xcassets/AppIcon.appiconset" "$assets/"

cd "$root"
INFO_PLIST="$here/Info.plist" ASSETS="$assets" CARGO_TARGET_DIR="$target" \
    exec bash "$bundler" --manifest-path "$root/apps/architect/Cargo.toml" --bin studio-architect "$@"
