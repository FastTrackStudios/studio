#!/usr/bin/env bash
# Bring one Cinematic Studio Strings section up to the 1st Violins standard,
# offline (no Kontakt needed):
#
#   features/rigs/orchestra/tests/css_section.sh "2nd Violins"
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

section="${1:?usage: css_section.sh <Section, e.g. \"2nd Violins\">}"
root="${CSS_ROOT:-/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings}"
nki_dir="/run/media/AudioHaven/Kontakt/Orchestral/Cinematic Studio Strings/Instruments"
nki_bin="${NKI_BIN:-/run/media/Development/FastTrackStudio-legacy/nkx-extract/target/release/nki}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
ksp="$root/_ksp/CSS $section"
patch="$root/_patches/$section"
stamp="$(date +%Y-%m-%d)"

[ -x "$nki_bin" ] || { echo "nki decoder not built: $nki_bin" >&2; exit 1; }
[ -f "$nki_dir/CSS $section.nki" ] || { echo "no NKI for $section" >&2; exit 1; }
[ -f "$patch/library.styx" ] || { echo "no $patch/library.styx" >&2; exit 1; }

# 1. decode (the key stays in this process's environment only)
eval "$(python3 -I - <<'PY'
import re
t = open("/run/media/AudioHaven/Kontakt/NativeAccess.xml", encoding="utf-8", errors="replace").read()
for p in re.split(r"(?=<Product )", t):
    m = re.search(r"<Name>([^<]*)</Name>", p)
    if m and m.group(1).strip().lower() == "cinematic studio strings":
        v = {k: re.search(rf"<{k}>([^<]*)</{k}>", p).group(1) for k in ("SNPID", "HU", "JDX")}
        print(f"SNPID={v['SNPID'].lstrip('0')}; HU={v['HU']}; JDX={v['JDX']}")
        break
PY
)"
[ -n "${JDX:-}" ] || { echo "no Native Access key for CSS" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
timeout 600 "$nki_bin" "$nki_dir/CSS $section.nki" --jdx "$JDX" --hu "$HU" --snpid "$SNPID" \
    --zones --out-dir "$tmp" >/dev/null 2>&1
unset JDX HU SNPID
[ -s "$tmp/zones.tsv" ] || { echo "decode failed" >&2; exit 1; }
mkdir -p "$ksp"
for f in zones.tsv groups.json groups.tsv persistent_0.tsv persistent_1.tsv script_0.ksp script_1.ksp script_2.ksp; do
    [ -f "$ksp/$f" ] && [ ! -f "$ksp/$f.pre-$stamp" ] && cp "$ksp/$f" "$ksp/$f.pre-$stamp"
    [ -f "$tmp/$f" ] && cp "$tmp/$f" "$ksp/$f"
done
echo "decoded: $(($(wc -l < "$ksp/zones.tsv") - 1)) zones → $ksp"

# 2. zones
[ -f "$patch/library.styx.pre-$stamp" ] || cp "$patch/library.styx" "$patch/library.styx.pre-$stamp"
(cd "$repo" && timeout 3000 nix develop -c cargo run -q --profile desktop-dev \
    --target x86_64-unknown-linux-gnu -p sampler --example signalpack -- \
    zones sync-tsv "$patch/library.styx" --from-tsv "$ksp/zones.tsv" --section "$section" \
    --disable-unmatched) \
    2>&1 | grep -E "matched|edited|wrote|^error" || true
# The other sections keep pizzicato in a "Pizz" folder; the articulation is
# Pizzicato everywhere (spec id, CC58 map).
sed -i -E 's/^(\s*articulation\s+)"Pizz"$/\1"Pizzicato"/' "$patch/library.styx"

# 3. the section's numbers
python3 -I "$here/css_section_spec.py" "$ksp" > "$patch/section.styx"
echo "wrote $patch/section.styx ($(grep -c '^        id ' "$patch/section.styx") articulations)"
