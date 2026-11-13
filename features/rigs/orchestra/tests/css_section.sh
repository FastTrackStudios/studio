#!/usr/bin/env bash
# Bring one Cinematic Studio instrument (a CSS strings section or a CSW
# woodwind) up to the 1st Violins standard, offline (no Kontakt needed):
#
#   features/rigs/orchestra/tests/css_section.sh "2nd Violins"
#   features/rigs/orchestra/tests/css_section.sh "CSW Solo Flute"   (full NKI name)
#   features/rigs/orchestra/tests/css_section.sh "CSB Solo Trumpet" (full NKI name)
#   DECODE_ONLY=1 …  decode into _ksp/ and stop (CSB Full Ensemble: no patch)
#
#   1. decode its NKI (zones with sample starts, groups, KSP, persistent
#      values) into _ksp/CSS <Section>/   (Native Access key from
#      NativeAccess.xml, never printed)
#   2. sync the zones into _patches/<Section>/library.styx (backed up)
#   3. write _patches/<Section>/section.styx — the section's decoded
#      numbers over the shared spec (css_section_spec.py): velocity tables,
#      RR, group curves, and the editor knobs that differ by section
#      (legato overlap delay, first-note fade and variant counter, marcato
#      offsets / LT volume / CC18, the overlay's velocity table)
#
# Then null-test it against Kontakt with the probe suite (null_suite.sh).
# Env: CSS_ROOT (default the AudioHaven CSS folder), NKI_BIN (nki decoder).
set -euo pipefail

section="${1:?usage: css_section.sh <Section, e.g. \"2nd Violins\" or \"CSW Solo Flute\">}"
nki_bin="${NKI_BIN:-/run/media/Development/FastTrackStudio-legacy/nkx-extract/target/release/nki}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
lib_root="/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series"
case "$section" in
  CSW\ *)
    # Woodwinds: the full NKI name; the patch / sample folder drops "CSW "
    # and a few solos use short codes. Samples sit flat (<mic>/<label>/<artic>).
    product="cinematic studio woodwinds"
    root="${CSS_ROOT:-$lib_root/Cinematic Studio Woodwinds}"
    nki="/run/media/AudioHaven/Kontakt/Orchestral/Cinematic Studio Woodwinds/Instruments/$section.nki"
    ksp="$root/_ksp/$section"
    case "$section" in
      "CSW Solo Alto Flute") label=Saf ;; "CSW Solo Bass Clarinet") label=Sbc ;;
      "CSW Solo Cor Anglais") label=Sca ;; "CSW Solo Contra Bassoon") label=Scb ;;
      "CSW Solo Piccolo") label=Spi ;; *) label="${section#CSW }" ;;
    esac
    flat=(--flat) ;;
  CSB\ *)
    # Brass: the full NKI name; the patch / sample folder drops "CSB " and
    # the horns and tuba are named in full. Samples sit flat like CSW's.
    product="cinematic studio brass"
    root="${CSS_ROOT:-$lib_root/Cinematic Studio Brass}"
    nki="/run/media/AudioHaven/Kontakt/Orchestral/Cinematic Studio Brass/Instruments/$section.nki"
    ksp="$root/_ksp/$section"
    case "$section" in
      "CSB 4 Horns") label="4 French Horns" ;; "CSB Solo Horn") label="Solo French Horn" ;;
      "CSB Solo Tuba") label=Tuba ;; *) label="${section#CSB }" ;;
    esac
    flat=(--flat) ;;
  *)
    product="cinematic studio strings"
    root="${CSS_ROOT:-$lib_root/Cinematic Studio Strings}"
    nki="/run/media/AudioHaven/Kontakt/Orchestral/Cinematic Studio Strings/Instruments/CSS $section.nki"
    ksp="$root/_ksp/CSS $section"
    label="$section"
    flat=() ;;
esac
patch="$root/_patches/$label"
stamp="$(date +%Y-%m-%d)"

[ -x "$nki_bin" ] || { echo "nki decoder not built: $nki_bin" >&2; exit 1; }
[ -f "$nki" ] || { echo "no NKI: $nki" >&2; exit 1; }
decode_only="${DECODE_ONLY:-}"
[ -n "$decode_only" ] || [ -f "$patch/library.styx" ] || { echo "no $patch/library.styx" >&2; exit 1; }

# 1. decode (the key stays in this process's environment only)
eval "$(PRODUCT="$product" python3 -I - <<'PY'
import os, re
t = open("/run/media/AudioHaven/Kontakt/NativeAccess.xml", encoding="utf-8", errors="replace").read()
for p in re.split(r"(?=<Product )", t):
    m = re.search(r"<Name>([^<]*)</Name>", p)
    if m and m.group(1).strip().lower() == os.environ["PRODUCT"]:
        v = {k: re.search(rf"<{k}>([^<]*)</{k}>", p).group(1) for k in ("SNPID", "HU", "JDX")}
        print(f"SNPID={v['SNPID'].lstrip('0')}; HU={v['HU']}; JDX={v['JDX']}")
        break
PY
)"
[ -n "${JDX:-}" ] || { echo "no Native Access key for $product" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
timeout 600 "$nki_bin" "$nki" --jdx "$JDX" --hu "$HU" --snpid "$SNPID" \
    --zones --out-dir "$tmp" >/dev/null 2>&1
unset JDX HU SNPID
[ -s "$tmp/zones.tsv" ] || { echo "decode failed" >&2; exit 1; }
mkdir -p "$ksp"
for f in zones.tsv groups.json groups.tsv persistent_0.tsv persistent_1.tsv script_0.ksp script_1.ksp script_2.ksp; do
    [ -f "$ksp/$f" ] && [ ! -f "$ksp/$f.pre-$stamp" ] && cp "$ksp/$f" "$ksp/$f.pre-$stamp"
    [ -f "$tmp/$f" ] && cp "$tmp/$f" "$ksp/$f"
done
echo "decoded: $(($(wc -l < "$ksp/zones.tsv") - 1)) zones → $ksp"
[ -z "$decode_only" ] || exit 0

# 2. zones
[ -f "$patch/library.styx.pre-$stamp" ] || cp "$patch/library.styx" "$patch/library.styx.pre-$stamp"
# Build first, on its own: a failed build must stop the run, not pass
# silently through the output filter below and sync nothing.
(cd "$repo" && timeout 3000 nix develop -c cargo build -q --profile desktop-dev \
    --target x86_64-unknown-linux-gnu -p sampler --example signalpack) \
    || { echo "signalpack build failed" >&2; exit 1; }
signalpack="$repo/target/x86_64-unknown-linux-gnu/desktop-dev/examples/signalpack"
sync_log="$(mktemp)"
timeout 1800 "$signalpack" zones sync-tsv "$patch/library.styx" --from-tsv "$ksp/zones.tsv" \
    --section "$label" "${flat[@]}" --disable-unmatched > "$sync_log" 2>&1 \
    || { cat "$sync_log" >&2; rm -f "$sync_log"; echo "sync-tsv failed" >&2; exit 1; }
grep -E "matched|edited|wrote|borrowed|unreachable" "$sync_log" || true
rm -f "$sync_log"
case "$section" in
  CSW\ *)
    # 3. the woodwind's numbers
    python3 -I "$here/csw_instrument_spec.py" "$ksp" > "$patch/section.styx"
    echo "wrote $patch/section.styx ($(grep -c '^        id ' "$patch/section.styx") articulations)"
    exit 0 ;;
  CSB\ *)
    echo "skipped section.styx (no brass overlay generator yet)"
    exit 0 ;;
esac
# The other sections keep pizzicato in a "Pizz" folder; the articulation is
# Pizzicato everywhere (spec id, CC58 map).
sed -i -E 's/^(\s*articulation\s+)"Pizz"$/\1"Pizzicato"/' "$patch/library.styx"

# 3. the section's numbers
python3 -I "$here/css_section_spec.py" "$ksp" > "$patch/section.styx"
echo "wrote $patch/section.styx ($(grep -c '^        id ' "$patch/section.styx") articulations)"
