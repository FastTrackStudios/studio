#!/usr/bin/env python3
"""Omnisphere's ring modulator (the oscillator's AM section) on a pure
sine, against ours: the strongest spectral lines each engine makes — their
frequency re the note and level re the plain sine's (AM off).

usage: amprobe.py "AMmoddepth=1,AMmodint=0.5,amwf=0,amkt=1,amscl=0.04,ampos=0.02" attr v1,v2,...
env: NOTE (48), OMNI_REFS, LINES (6)
"""
import os, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves
os.environ.setdefault("NOTE", "48")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lfoprobe as L  # noqa: E402
import shapecurve as S  # noqa: E402

SR = 48000
LINES = int(os.environ.get("LINES", "6"))


def lines(x, ref):
    seg = x[int(0.5 * SR):int(1.9 * SR)]
    sp = np.abs(np.fft.rfft(seg * np.hanning(len(seg))))
    fr = np.fft.rfftfreq(len(seg), 1 / SR)
    pk = [i for i in range(3, len(sp) - 3) if fr[i] < 8000 and sp[i] == sp[i - 3:i + 4].max()]
    pk.sort(key=lambda i: -sp[i])
    return [(fr[i], 20 * np.log10(sp[i] / ref + 1e-12)) for i in pk[:LINES]]


def main():
    base = {k: float(v) for k, v in (kv.split("=") for kv in sys.argv[1].split(","))}
    attr, values = sys.argv[2], [float(v) for v in sys.argv[3].split(",")]
    f0 = 440 * 2 ** ((float(os.environ["NOTE"]) - 69) / 12)
    plain = S.base_xml()
    refs = {}
    for side, ours in (("omni", False), ("ours", True)):
        seg = L.render(plain, ours)[int(0.5 * SR):int(1.9 * SR)]
        refs[side] = np.abs(np.fft.rfft(seg * np.hanning(len(seg)))).max()
    for v in values:
        xml = S.edit(plain, "OSC", {"am": S.hexf(1.0), **{k: S.hexf(x) for k, x in {**base, attr: v}.items()}})
        print(f"{attr}={v}")
        for side, ours in (("omni", False), ("ours", True)):
            ls = sorted(lines(L.render(xml, ours), refs[side]))
            print(f"  {side} " + "  ".join(f"{f / f0:5.2f}:{db:+5.1f}" for f, db in ls), flush=True)


if __name__ == "__main__":
    main()
