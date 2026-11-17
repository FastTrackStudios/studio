#!/usr/bin/env python3
"""MOJO 2 probe MIDI + manifest for the Kontakt null test.

    python3 mojo_probe.py <mode> <instrument> <out.mid> <out_manifest.tsv> [--root DIR]

Same timing and manifest as gen_probe.py (120 BPM, 600 PPQ; units isolated
by silence, windowed by null_test.py at the same frames). Each articulation
block starts with its keyswitch (Key mode, `%jqdqz`: C1 = MIDI 24 …), then
one unit per (key, velocity). Keys are spread over that articulation's
shipped range (`key_range` in the instrument's section.styx), so no unit
falls outside it.

Modes:
  oneshots  Staccato and Stabs at velocities across all four layers
            (20 50 80 110 127), the one-layer FX at 60 and 110; three keys each.
  stacc     Staccato only (the first null).
"""
import os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_probe import Smf  # noqa: E402  (same SMF writer and tick grid)

KEYSWITCH = {"Sus": 24, "Stacc": 25, "Stabs": 26, "BendDown": 27, "RunDown": 28, "RunUp": 29,
             "Rise": 30, "Doits": 33, "Shakes": 34, "Trills": 35, "Falls": 36}
LAYERED = (20, 50, 80, 110, 127)
ONE_LAYER = (60, 110)
MODES = {
    "stacc": [("Stacc", LAYERED)],
    "oneshots": [("Stacc", LAYERED), ("Stabs", LAYERED)]
    + [(a, ONE_LAYER) for a in ("BendDown", "RunDown", "RunUp", "Doits", "Rise", "Shakes", "Trills", "Falls")],
}
HOLD = 2.0   # key held this long (every one-shot sample here ends sooner or rings under the release)
WIN = 3.5    # window scored from the note-on
GAP = 1.5    # silence after the window


def key_ranges(section_styx):
    """`id` → (lo, hi) from an instrument's section.styx."""
    out, cur = {}, None
    for line in open(section_styx):
        m = re.match(r"\s*id (\S+)", line)
        if m:
            cur = m.group(1)
        m = re.match(r"\s*key_range \((\d+) (\d+)\)", line)
        if m and cur:
            out[cur] = (int(m.group(1)), int(m.group(2)))
    return out


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 4 or args[0] not in MODES:
        raise SystemExit(__doc__)
    mode, inst, mid, manifest = args
    root = next((a.split("=", 1)[1] for a in sys.argv if a.startswith("--root=")),
                "/run/media/AudioHaven/Sampled/Horns/MOJO 2 Horn Section")
    ranges = key_ranges(os.path.join(root, "_patches", inst, "section.styx"))
    m, rows, t = Smf(), [], 1.0
    for art, vels in MODES[mode]:
        lo, hi = ranges[art]
        keys = sorted({lo + 2, (lo + hi) // 2, hi - 3})
        m.note(t, 0.1, KEYSWITCH[art], 100)
        t += 1.0
        for key in keys:
            for vel in vels:
                m.note(t, HOLD, key, vel)
                rows.append((len(rows), f"{t:.3f}", f"{WIN:.3f}", key, art.upper(), f"{art} key{key} v{vel}"))
                t += WIN + GAP
    m.write(mid)
    with open(manifest, "w") as f:
        f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
        for r in rows:
            f.write("\t".join(map(str, r)) + "\n")
    print(f"{len(rows)} units, {t:.1f} s")


if __name__ == "__main__":
    main()
