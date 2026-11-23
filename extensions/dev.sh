#!/usr/bin/env bash
# Studio's engine and views as Architect extensions, in a stock Architect
# as development copies: builds their programs (the root workspace's
# `desktop-dev` profile for the host, as the app recipes build, so
# the shared crates are compiled once), installs each into its package
# (its manifest's [entry]), and starts an isolated Architect
# (`architect-desktop dev`) with only them and the playhead on. Extra
# arguments go to `dev` (--hide all, --run …).
#
#   ARCHITECT=…/architect-desktop FTS_DAW_OPEN="song.rpp" extensions/dev.sh --hide all
#
#   EXTENSIONS="daw tempo-map" extensions/dev.sh --hide all    # tempo mapping
#
# A package is watched while the app runs: build a program again and
# install it over the package's (`install -m 755 target/<host>/desktop-dev/
# studio-tempo-map extensions/tempo-map/`), and the running app restarts
# that extension alone (how studio.tempo-map's algorithms are swapped).
#
# FTS_DAW_OPEN is what the engine opens (a song, a .session, a setlist).
# PIPEWIRE_RUNTIME_DIR, when set, is where the engine finds its audio
# server (a private one with a null sink keeps a test off real speakers).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
packages=${EXTENSIONS:-"daw arrangement mixer"}
declare -A program=([daw]=daw-extension [arrangement]=studio-arrangement [mixer]=studio-mixer [tempo-map]=studio-tempo-map)
declare -A id=([daw]=daw [arrangement]=studio.arrangement [mixer]=studio.mixer [tempo-map]=studio.tempo-map)
host=$(rustc -vV | sed -n 's/^host: //p')
build=() only=playhead args=()
for p in $packages; do
  build+=(-p "${program[$p]}")
  only+=",${id[$p]}"
  args+=(--extension "$here/$p")
done
cargo build --manifest-path "$root/Cargo.toml" --profile desktop-dev --target "$host" "${build[@]}"
for p in $packages; do
  install -m 755 "$root/target/$host/desktop-dev/${program[$p]}" "$here/$p/${program[$p]}"
done
exec "${ARCHITECT:-architect-desktop}" dev --name "${ARCH_DEV_NAME:-studio-extensions}" "${args[@]}" --only "$only" "$@"
