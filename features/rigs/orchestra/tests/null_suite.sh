#!/usr/bin/env bash
# Null-test one CSS section against Kontakt, probe by probe:
#
#   features/rigs/orchestra/tests/null_suite.sh "Violas" [mode ...]
#
# For each mode (default: all of them) it writes the probe MIDI
# (gen_probe.py, transposed into the section's range), renders it twice
# through the section's factory Kontakt chain on voyager (two runs tell
# Kontakt's random draws from deterministic notes), renders ours
# (render_css_live --section) and scores the pair (null_test.py).
#
#   modes: probe legato leg2 marcato marc2 shorts pizz trem rebow attack
#
# The Kontakt chain is target/orchestra-ab/chains/fxchain_<tag>.txt — the
# track <FXCHAIN> of a REAPER project with the factory CSS <Section> loaded
# (see css_section.sh and the css-section-process notes). Kontakt renders
# run in a separate headless REAPER (-newinst), never in the user's session.
# Shorts and pizzicato draw a random detune, zone-2 legato (leg2) a random
# body variant: those modes fit Kontakt's draws first (fit_draws.py) and
# replay them, and are scored against the first Kontakt run only.
#
# Env: KONTAKT_HOST (default voyager), SKIP_KONTAKT=1 reuses existing renders,
# BIN another render_css_live.
set -euo pipefail

section="${1:?usage: null_suite.sh <Section> [mode ...]}"
shift
case "$section" in
  "1st Violins") tag=1v; tr=0 ;;
  "2nd Violins") tag=2v; tr=0 ;;
  "Violas")      tag=va; tr=-7 ;;
  "Cellos")      tag=vc; tr=-19 ;;
  "Basses")      tag=cb; tr=-24 ;;
  *) echo "unknown section: $section" >&2; exit 1 ;;
esac
modes=("$@")
[ ${#modes[@]} -gt 0 ] || modes=(probe legato leg2 marcato marc2 shorts pizz trem rebow attack)

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
host="${KONTAKT_HOST:-voyager}"
chain="$repo/target/orchestra-ab/chains/fxchain_$tag.txt"
work="$repo/target/orchestra-ab/sec/$tag"
bin="${BIN:-$repo/target/x86_64-unknown-linux-gnu/desktop-dev/examples/render_css_live}"
[ -f "$chain" ] || { echo "no Kontakt chain: $chain" >&2; exit 1; }
[ -x "$bin" ] || { echo "render_css_live not built: $bin" >&2; exit 1; }
# shellcheck disable=SC2016  # $HOME is the remote one
remote_home="$(timeout 20 ssh "$host" 'echo $HOME')"
remote="$remote_home/orchestra-ab/sec/$tag"
timeout 20 ssh "$host" "mkdir -p '$remote'"

# python with numpy (null_test.py): the system one if it has it, else nix
npy() {
  if python3 -c "import numpy" 2>/dev/null; then
    python3 -I "$@"
  else
    timeout 3000 nix-shell -p 'python3.withPackages (ps: [ ps.numpy ])' --run "python3 -I $(printf '%q ' "$@")"
  fi
}

flags_for() {
  case "$1" in
    probe) echo "" ;;
    legato) echo "--legato" ;;
    leg2) echo "--legato --zone2" ;;
    marcato) echo "--marcato" ;;
    marc2) echo "--marcato --overlay" ;;
    shorts) echo "--shorts" ;;
    pizz) echo "--shorts --pizz" ;;
    trem) echo "--tremolo" ;;
    rebow) echo "--tremolo --rebow" ;;
    attack) echo "--attack" ;;
    *) echo "unknown mode: $1" >&2; return 1 ;;
  esac
}

for mode in "${modes[@]}"; do
  d="$work/$mode"
  mkdir -p "$d"
  read -r -a fl <<< "$(flags_for "$mode")"
  python3 -I "$here/gen_probe.py" "$d/$mode.mid" "$d/${mode}_manifest.tsv" "${fl[@]}" "--transpose=$tr" >/dev/null

  if [ -z "${SKIP_KONTAKT:-}" ] || [ ! -f "$d/${mode}_kontakt_r2.wav" ]; then
    for r in r1 r2; do
      python3 -I "$here/make_ab_rpp.py" "$chain" "$d/$mode.mid" \
        "$remote/${mode}_kontakt_$r.wav" "$d/${mode}_$r.rpp" >/dev/null
      timeout 120 scp -q "$d/${mode}_$r.rpp" "$host:$remote/"
      # A headless REAPER of its own: renders, then quits.
      timeout 900 ssh "$host" "/Applications/REAPER.app/Contents/MacOS/REAPER -newinst -nosplash \
        -renderproject '$remote/${mode}_$r.rpp' >/dev/null 2>&1" || true
      timeout 120 scp -q "$host:$remote/${mode}_kontakt_$r.wav" "$d/"
    done
  fi

  echo "== $section $mode"
  case "$mode" in
    shorts|pizz|leg2|attack)
      kind=--tunes
      [ "$mode" = leg2 ] && kind=--variants
      [ "$mode" = attack ] && kind=--attacks
      npy "$here/fit_draws.py" "$kind" "$bin" "$section" "$d/$mode.mid" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" --json "$d/null.json" | tail -1
      ;;
    *)
      "$bin" "$d/$mode.mid" "$d/${mode}_ours.wav" --section "$section" >/dev/null 2>&1
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_kontakt_r2.wav" --json "$d/null.json" | tail -2
      ;;
  esac
done
