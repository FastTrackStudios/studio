#!/usr/bin/env bash
# Session's parameters for the shared scripts/apple/deploy-iphone.sh (STA-393).
# Everything else lives in the shared script; scripts/apple/README.md lists the
# parameters. Every value below is a default: the environment overrides it.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"

export DX_APP_DIR="${DX_APP_DIR:-apps/session/desktop}"
export DX_PACKAGE="${DX_PACKAGE:-session-desktop}"
export DX_FEATURES="${DX_FEATURES:-session-domain,charts}"
export IOS_APP_NAME="${IOS_APP_NAME:-Session-desktop}"
# cpal's duplex audio session can request mic access even when only output is used.
export MIC_USAGE="${MIC_USAGE:-Used for the audio session during setlist playback.}"
exec bash "$ROOT/scripts/apple/deploy-iphone.sh" "$@"
