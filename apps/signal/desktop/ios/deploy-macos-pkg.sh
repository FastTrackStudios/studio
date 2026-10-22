#!/usr/bin/env bash
# Signal's parameters for the shared scripts/apple/deploy-macos-pkg.sh (STA-393).
# Everything else lives in the shared script; scripts/apple/README.md lists the
# parameters. Every value below is a default: the environment overrides it.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"

# The pkg builds the app through this directory's deploy-macos.sh wrapper.
export APP_IOS_DIR="${APP_IOS_DIR:-$HERE}"
# Crate whose version names the .pkg when MARKETING_VER is unset.
export VERSION_PACKAGE="${VERSION_PACKAGE:-signal-desktop}"
exec bash "$ROOT/scripts/apple/deploy-macos-pkg.sh" "$@"
