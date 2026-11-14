#!/usr/bin/env bash
# Null-test one Cinematic Studio instrument against Kontakt, probe by probe:
#
#   features/rigs/orchestra/tests/null_suite.sh "Violas" [mode ...]
#   features/rigs/orchestra/tests/null_suite.sh "Solo Flute" [mode ...]
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
# section → chain tag, probe transposition (into the instrument's range) and
# library; the section is the instrument's patch folder name.
lib="strings"
case "$section" in
  "1st Violins") tag=1v; tr=0 ;;
  "2nd Violins") tag=2v; tr=0 ;;
  "Violas")      tag=va; tr=-7 ;;
  "Cellos")      tag=vc; tr=-19 ;;
  "Basses")      tag=cb; tr=-24 ;;
  "2 Bassoons")    lib=woodwinds; tag=ww_bn2; tr=-7 ;;
  "2 Clarinets")   lib=woodwinds; tag=ww_cl2; tr=0 ;;
  "2 Flutes")      lib=woodwinds; tag=ww_fl2; tr=0 ;;
  "2 Oboes")       lib=woodwinds; tag=ww_ob2; tr=0 ;;
  "Saf")           lib=woodwinds; tag=ww_afl; tr=0 ;;
  "Sbc")           lib=woodwinds; tag=ww_bcl; tr=-7 ;;
  "Solo Bassoon")  lib=woodwinds; tag=ww_bn;  tr=-7 ;;
  "Solo Clarinet") lib=woodwinds; tag=ww_cl;  tr=0 ;;
  "Scb")           lib=woodwinds; tag=ww_cbn; tr=-11 ;;
  "Sca")           lib=woodwinds; tag=ww_ca;  tr=0 ;;
  "Solo Flute")    lib=woodwinds; tag=ww_fl;  tr=0 ;;
  "Solo Oboe")     lib=woodwinds; tag=ww_ob;  tr=0 ;;
  "Spi")           lib=woodwinds; tag=ww_picc; tr=0 ;;
  "2 Trombones")        lib=brass; tag=br_tb2;  tr=-10 ;;
  "2 Trumpets")         lib=brass; tag=br_tpt2; tr=0 ;;
  "4 French Horns")     lib=brass; tag=br_hn4;  tr=-3 ;;
  "Solo Bass Trombone") lib=brass; tag=br_btb;  tr=-10 ;;
  "Solo French Horn")   lib=brass; tag=br_hn;   tr=-3 ;;
  "Solo Trombone")      lib=brass; tag=br_tb;   tr=-10 ;;
  "Solo Trumpet")       lib=brass; tag=br_tpt;  tr=0 ;;
  "Tuba")               lib=brass; tag=br_tuba; tr=-11 ;;
  *) echo "unknown section: $section" >&2; exit 1 ;;
esac
export CS_LIBRARY="$lib"   # fit_draws.py passes it on to the renderer
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
# The Kontakt host is reached only when a render is missing (SKIP_KONTAKT
# with every render in place works with voyager away).
remote=""
cfg=""
remote_dir() {
  if [ -z "$remote" ]; then
    local home
    # shellcheck disable=SC2016  # $HOME is the remote one
    home="$(timeout 20 ssh "$host" 'echo $HOME')"
    [ -n "$home" ] || { echo "cannot reach $host" >&2; exit 1; }
    remote="$home/orchestra-ab/sec/$tag"
    # A render REAPER rewrites its ini on the way out, cut down to a stub: it
    # must never run on the user's reaper.ini. Each section renders on its own
    # copy of the user's settings (~/orchestra-ab/cfg/user-reaper.ini, with the
    # plugin caches beside it), passed with -cfgfile.
    cfg="$home/orchestra-ab/cfg/render-$tag.ini"
    timeout 20 ssh "$host" "mkdir -p '$remote' && cp ~/orchestra-ab/cfg/user-reaper.ini '$cfg'"
  fi
}

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
    remote_dir
    for r in r1 r2; do
      python3 -I "$here/make_ab_rpp.py" "$chain" "$d/$mode.mid" \
        "$remote/${mode}_kontakt_$r.wav" "$d/${mode}_$r.rpp" >/dev/null
      timeout 120 scp -q "$d/${mode}_$r.rpp" "$host:$remote/"
      # A headless REAPER of its own: renders, then quits.
      # REAPER may return before the render is done: wait for it to exit
      timeout 900 ssh "$host" "/Applications/REAPER.app/Contents/MacOS/REAPER -newinst -nosplash \
        -cfgfile '$cfg' -renderproject '$remote/${mode}_$r.rpp' >/dev/null 2>&1
        for i in \$(seq 280); do
          pgrep -f '^/Applications/REAPER.app/Contents/MacOS/REAPER .*/${mode}_${r}[.]rpp' >/dev/null || break
          sleep 3
        done" || true
      timeout 120 scp -q "$host:$remote/${mode}_kontakt_$r.wav" "$d/"
    done
  fi

  echo "== $section $mode"
  case "$mode" in
    shorts|pizz|leg2|attack)
      kind=--tunes
      # woodwinds / brass draw their shorts' round robins at random (and
      # do not detune them): fit the RR each note drew
      if [ "$lib" != strings ] && { [ "$mode" = shorts ] || [ "$mode" = pizz ]; }; then
        kind=--rrs
      fi
      [ "$mode" = leg2 ] && kind=--variants
      [ "$mode" = attack ] && kind=--attacks
      npy "$here/fit_draws.py" "$kind" "$bin" "$section" "$d/$mode.mid" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" --json "$d/null.json" | tail -1
      ;;
    *)
      "$bin" "$d/$mode.mid" "$d/${mode}_ours.wav" --section "$section" --library "$lib" >/dev/null 2>&1
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_kontakt_r2.wav" --json "$d/null.json" | tail -2
      ;;
  esac
done
