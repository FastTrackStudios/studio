#!/usr/bin/env bash
# Put frame's generated faces on the connected Android device (or emulator),
# where signal-mobile reads them: its external files, `frame/`.
#
#   apps/signal/mobile/scripts/push-faces.sh [FACES_DIR]
#
# FACES_DIR defaults to this repo's features/ui/plugin-faces/generated
# (`just faces`).
set -euo pipefail
SRC="${1:-$(cd "$(dirname "$0")/../../../.." && pwd)/features/ui/plugin-faces/generated}"
DEST=/sdcard/Android/data/app.fasttrackstudio.signal/files/frame
adb shell mkdir -p "$DEST"
adb push --sync "$SRC/rig-faces" "$DEST/" >/dev/null
echo "faces → $DEST/rig-faces"
