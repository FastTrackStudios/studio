#!/usr/bin/env python3
"""What each parameter of one Omnisphere effect does: hold a base set, move
one `Pn` at a time across 0..1, render through real Omnisphere (the init
part, held note, unit in the first Common FX slot) and print what changed
against the base — level, spectrum (RMS and tilt), stereo width, level
swing over the held note, tail after the key-up. `BANDS=1` adds the
1/3-octave change for every step. A parameter that moves nothing is unused
or UI-only.

usage: fxsweep.py "<Unit>" [P0=v,P3=v,...]   env: NOTE (36), BANDS, STEPS
"""
import os, sys
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("NOTE", "36")
import fxdelta as F  # noqa: E402

STEPS = [float(v) for v in os.environ.get("STEPS", "0,0.25,0.5,0.75,1").split(",")]


def main():
    unit = sys.argv[1]
    base = [0.0] * 15
    for kv in (sys.argv[2].split(",") if len(sys.argv) > 2 and sys.argv[2] else []):
        k, v = kv.split("=")
        base[int(k.lstrip("P"))] = float(v)
    init = open(os.path.join(F.HERE, "init_part.prt_omn"), encoding="latin-1").read()
    os.makedirs(F.REFS, exist_ok=True)
    ref = F.measure(F.render(F.with_fx(init, unit, [F.hexf(v) for v in base])))
    ok = ref["bands"] > ref["bands"].max() - 40
    print(f"{unit}  base {base}  level {ref['level']:.1f} dB")
    print("param  value  level   spec   tilt   width  swing  tail")
    for p in range(15):
        for v in STEPS:
            if abs(v - base[p]) < 1e-6:
                continue
            ps = list(base)
            ps[p] = v
            m = F.measure(F.render(F.with_fx(init, unit, [F.hexf(x) for x in ps])))
            dl = m["level"] - ref["level"]
            d = m["bands"] - ref["bands"] - dl
            spec = float(np.sqrt(np.mean(d[ok] ** 2)))
            tilt = float(np.polyfit(np.log2(F.EDGES[:-1][ok]), d[ok], 1)[0])
            print(f"P{p:<4} {v:5.2f}  {dl:+6.1f} {spec:6.1f} {tilt:+6.1f} {m['side']-ref['side']:+6.1f}"
                  f" {m['swing']-ref['swing']:+6.1f} {m['tail']-ref['tail']:+6.1f}", flush=True)
            if os.environ.get("BANDS"):
                print("        " + " ".join(f"{x:+.0f}" for x in d[ok]))


if __name__ == "__main__":
    main()
