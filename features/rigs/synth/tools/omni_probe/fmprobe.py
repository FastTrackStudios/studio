#!/usr/bin/env python3
"""Omnisphere's FM on a pure sine, against ours: the carrier is the init
part's oscillator playing a sine, its FM section on; prints, per setting,
the modulator frequency (re the note) and the FM index both engines show —
read off the sideband levels (Bessel J1/J0 on the strongest pair).

usage: fmprobe.py "moddepth=v,modint=v,fmwf=0,fmkt=1,fmscl=0" attr v1,v2,...
env: NOTE (48), OMNI_REFS
"""
import os, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves
os.environ.setdefault("NOTE", "48")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lfoprobe as L  # noqa: E402
import shapecurve as S  # noqa: E402

SR = 48000


def bessel_index(r):
    """β with J1(β)/J0(β) = r (β < 2.4, before J0's first zero)."""
    from math import factorial
    def J(n, b):
        return sum((-1) ** m / (factorial(m) * factorial(m + n)) * (b / 2) ** (2 * m + n) for m in range(30))
    bs = np.linspace(0.0, 2.3, 2301)
    return float(bs[np.argmin([abs(J(1, b) / max(J(0, b), 1e-9) - r) for b in bs])])


def analyse(x, f0):
    seg = x[int(0.5 * SR):int(1.9 * SR)]
    sp = np.abs(np.fft.rfft(seg * np.hanning(len(seg))))
    fr = np.fft.rfftfreq(len(seg), 1 / SR)
    k0 = np.argmin(abs(fr - f0))
    c = sp[k0 - 3:k0 + 4].max()
    # The strongest line off the harmonics below 4·f0: f0 ± fm — so measure at
    # a non-integer ratio.
    # (Away from the harmonics: the sampled sine carries a few of its own.)
    near_h = abs(fr / f0 - np.round(fr / f0)) * f0 < 3
    mask = (fr > 10) & (fr < 4 * f0) & ~near_h
    k = np.flatnonzero(mask)[np.argmax(sp[mask])]
    side = sp[k]
    return abs(fr[k] - f0) / f0, 20 * np.log10(side / c), bessel_index(side / c)


def main():
    base = {k: float(v) for k, v in (kv.split("=") for kv in sys.argv[1].split(","))}
    attr, values = sys.argv[2], [float(v) for v in sys.argv[3].split(",")]
    f0 = 440 * 2 ** ((float(os.environ["NOTE"]) - 69) / 12)
    for v in values:
        xml = S.base_xml()
        xml = S.edit(xml, "OSC", {"fm": S.hexf(1.0), **{k: S.hexf(x) for k, x in {**base, attr: v}.items()}})
        out = []
        for side, ours in (("omni", False), ("ours", True)):
            r, db, beta = analyse(L.render(xml, ours), f0)
            out.append(f"{side} fm/f0 {r:5.2f}  side {db:+6.1f} dB  β {beta:4.2f}")
        print(f"{attr}={v:<6} " + "   ".join(out), flush=True)


if __name__ == "__main__":
    main()
