#!/bin/bash
# Wait for Logic's Stem Splitter output, then copy it to a reference folder.
# usage: collect_stems.sh "<project>.logicx" "<source .wav that was imported>" "<dest folder>" [timeout_s]
# Logic writes stems to <project>/Media/Audio Files/<basename>_<Stem>.wav while it splits.
# Copies become "<basename> (<Stem>).wav", the naming of the earlier reference folders.
set -euo pipefail
PROJ=$1; SRCWAV=$2; DEST=$3; TIMEOUT=${4:-600}
BASE=$(basename "$SRCWAV" .wav)
SRC="$PROJ/Media/Audio Files"
STEMS=(Vocals Bass Guitar Piano Other Kick Snare Hi-Hat Toms Cymbals "Drums Other")

sizes() { for s in "${STEMS[@]}"; do stat -f '%z' "$SRC/${BASE}_$s.wav" 2>/dev/null || echo missing; done | tr '\n' ' '; }

start=$(date +%s); prev=""
while :; do
  cur=$(sizes)
  if [[ $cur != *missing* && $cur == "$prev" ]]; then break; fi   # all present, sizes unchanged for one interval
  if (( $(date +%s) - start > TIMEOUT )); then echo "timed out; sizes: $cur" >&2; exit 1; fi
  prev=$cur; sleep 5
done

mkdir -p "$DEST"
for s in "${STEMS[@]}"; do
  cp -n "$SRC/${BASE}_$s.wav" "$DEST/$BASE ($s).wav"
done

src_len=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$SRCWAV")
echo "source length: $src_len s"
for s in "${STEMS[@]}"; do
  f="$DEST/$BASE ($s).wav"
  len=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$f")
  vol=$(ffmpeg -nostats -i "$f" -af volumedetect -f null - 2>&1 | grep -o 'mean_volume: .*' || true)
  printf '%-12s %s s  %s\n' "$s" "$len" "$vol"
done
