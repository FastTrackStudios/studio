#!/usr/bin/env bash
# Signal's parameters for the shared scripts/apple/deploy-macos.sh (STA-393).
# Everything else lives in the shared script; scripts/apple/README.md lists the
# parameters. Every value below is a default: the environment overrides it.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"

export DX_PACKAGE="${DX_PACKAGE:-signal-desktop}"
export DX_APP_DIR="${DX_APP_DIR:-apps/signal/desktop}"
export ICONS_DIR="${ICONS_DIR:-$ROOT/$DX_APP_DIR/ios/Assets.xcassets}"
# DMG name when MARKETING_VER is unset.
export DMG_FALLBACK_VER="${DMG_FALLBACK_VER:-0.0.1}"
exec bash "$ROOT/scripts/apple/deploy-macos.sh" "$@"
