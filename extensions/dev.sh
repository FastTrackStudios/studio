#!/usr/bin/env bash
# The extensions in a stock Architect, as development copies: builds their
# programs, installs each into its package (its manifest's [entry]), and
# starts an isolated Architect (`architect-desktop dev`) with only them and
# the playhead on. Extra arguments go to `dev` (--hide all, --run …).
#
#   ARCHITECT=…/architect-desktop FTS_DAW_OPEN=song.session extensions/dev.sh --hide all
#
# PIPEWIRE_RUNTIME_DIR, when set, is where the engine finds its audio
# server (a private one with a null sink keeps a test off real speakers).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
packages=${EXTENSIONS:-"daw arrangement mixer"}
declare -A program=([daw]=daw-extension [arrangement]=studio-arrangement [mixer]=studio-mixer)
declare -A id=([daw]=daw [arrangement]=studio.arrangement [mixer]=studio.mixer)
build=() only=playhead args=()
for p in $packages; do
  build+=(-p "${program[$p]}")
  only+=",${id[$p]}"
  args+=(--extension "$here/$p")
done
cargo build --manifest-path "$here/Cargo.toml" "${build[@]}"
for p in $packages; do
  install -m 755 "$here/target/debug/${program[$p]}" "$here/$p/${program[$p]}"
done
exec "${ARCHITECT:-architect-desktop}" dev --name "${ARCH_DEV_NAME:-studio-extensions}" "${args[@]}" --only "$only" "$@"
