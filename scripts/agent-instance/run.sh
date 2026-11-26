#!/usr/bin/env bash
# A second Studio for an agent (Linux): instance "agent", its own runtime dir
# (own engine, sockets, discovery), the desktop's display, bus and audio.
# Build first (the engine, then the window, as `just studio` builds them):
#   scripts/dev-shell.sh -c 'cargo build --profile desktop-dev --target "$(uname -m)-unknown-linux-gnu" -p daw-extension --bin daw-extension'
#   cd apps/studio && dx build --platform desktop --renderer native --no-default-features --features native --profile desktop-dev
#   scripts/dev-shell.sh scripts/agent-instance/run.sh
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
uid_rt=/run/user/$(id -u)
mkdir -p "$uid_rt/agent-rt" && chmod 0700 "$uid_rt/agent-rt"
export XDG_RUNTIME_DIR=$uid_rt/agent-rt
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-$uid_rt/wayland-1}
[[ $WAYLAND_DISPLAY == /* ]] || WAYLAND_DISPLAY=$uid_rt/$WAYLAND_DISPLAY
export DISPLAY=${DISPLAY:-:0}
export DBUS_SESSION_BUS_ADDRESS=unix:path=$uid_rt/bus
export PIPEWIRE_RUNTIME_DIR=$uid_rt
export PULSE_SERVER=unix:$uid_rt/pulse/native
export STUDIO_INSTANCE=${STUDIO_INSTANCE:-agent}
export STUDIO_SINGLE_WINDOW=${STUDIO_SINGLE_WINDOW:-1}
host=$(uname -m)-unknown-linux-gnu
export FTS_DAW_ENGINE_BIN=$root/target/$host/desktop-dev/daw-extension
export STUDIO_KEEP_ENGINE=0
export RUST_LOG=${RUST_LOG:-warn}
cd "$root/target/dx/Studio/debug/linux/app"
exec ./Studio
