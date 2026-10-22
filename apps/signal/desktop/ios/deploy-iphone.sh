#!/usr/bin/env bash
# Signal's parameters for the shared scripts/apple/deploy-iphone.sh (STA-393).
# Everything else lives in the shared script; scripts/apple/README.md lists the
# parameters. Every value below is a default: the environment overrides it.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"

export DX_APP_DIR="${DX_APP_DIR:-apps/signal/desktop}"
export DX_PACKAGE="${DX_PACKAGE:-signal-desktop}"
export DX_FEATURES="${DX_FEATURES:-signal-guitar,signal-keys-rig,signal-drums-rig,signal-ekit-rig}"
export IOS_APP_NAME="${IOS_APP_NAME:-Signal-desktop}"
# Runtime per-screen rotation (ios_orientation.rs) needs both orientations, which dx already emits.
export MIC_USAGE="${MIC_USAGE:-Processes your guitar signal from the connected audio interface or microphone.}"
exec bash "$ROOT/scripts/apple/deploy-iphone.sh" "$@"
