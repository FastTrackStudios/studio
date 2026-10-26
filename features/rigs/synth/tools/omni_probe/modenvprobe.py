#!/usr/bin/env python3
"""Omnisphere's Mod Env 1, read off a pure sine, against ours.

Writes the envelope's points into the init part, routes ModEnv1 to the
fine tune (a 100 cent swing) and recovers the pitch over the held note on
both sides (lfoprobe.py's tracker).

usage: modenvprobe.py curve C...   one 1 s decay per curve value C, the
                                   fitted exponent per side
       modenvprobe.py run          loop / hold cases, the level every 0.4 s
env: OMNI_REFS, SYNC=1 runs the envelope in beats
"""
import os, re, sys
import numpy as np

os.environ.setdefault("HOLD", "4.0")
os.environ.setdefault("SOURCE", "ModEnv1")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lfoprobe as L  # noqa: E402
import shapecurve as S  # noqa: E402

ON = "3f800000"


def with_points(pts, lp="0"):
    """pts: (level, t, curve, s); `c` on MODENV is set to the live count
    (Omnisphere keeps one spare `p` past them, so one is appended)."""
    xml = L.preset("pitch", {"rate": 0.4}, {"lo": 0, "hi": 0.5})
    a = xml.index("<MODENV ")
    b = xml.index("</MODENV>", a)
    head = re.sub(r'\sc="\d+"', f' c="{len(pts)}"', re.search(r"<MODENV[^>]*>", xml[a:b]).group(0))
    spare = (0.0, pts[-1][1] + 0.01, 0.5, "11")
    body = "".join(f'<p l="{S.hexf(l)}"  t="{S.hexf(t)}"  s="{s}"  c="{S.hexf(c)}" ></p> '
                   for l, t, c, s in [*pts, spare])
    xml = xml[:a] + head + body + xml[b:]
    m = re.search(r"<MODENVPARAMS[^>]*>", xml)
    tag = re.sub(r'(\slp=)"[^"]*"', r'\1"%s"' % lp, m.group(0))
    tag = re.sub(r'(\ssync=)"[^"]*"', r'\1"%s"' % (ON if os.environ.get("SYNC") else "0"), tag)
    return xml[:m.start()] + tag + xml[m.end():]


def curve(cs):
    xs = np.linspace(0.05, 0.95, 19)
    es = np.exp(np.linspace(0, 3.5, 400))
    for c in cs:
        xml = with_points([(0, 0, 0.5, "18"), (1, 0, c, "18"), (0, 0.01, 0.5, "18")])
        out = []
        for side, ours in (("omni", False), ("ours", True)):
            y = L.trace(L.render(xml, ours), "pitch")
            base = np.median(y[1500:1900])
            v = (y[(xs * 1000).astype(int) + 3] - base) / (np.median(y[8:14]) - base)
            law = (lambda e: (1 - xs) ** e) if c < 0.5 else (lambda e: 1 - xs ** e)
            err, e = min((np.sum((v - law(e)) ** 2), e) for e in es)
            out.append(f"{side} e {e:6.2f} (rms {np.sqrt(err / len(xs)):.3f})")
        print(f"c={c:.3f}  " + "   ".join(out), flush=True)


def run():
    cases = {
        "hold 1>.5@1s": ([(0, 0, .5, "18"), (1, 0, .5, "18"), (.5, .01, .5, "18")], "0"),
        "loop 1>.5@1s": ([(0, 0, .5, "18"), (1, 0, .5, "18"), (.5, .01, .5, "18")], ON),
        "loop 1>0@2s": ([(0, 0, .5, "18"), (1, 0, .5, "18"), (0, .02, .5, "18")], ON),
        "loop up .5s, down 2s": ([(0, 0, .5, "18"), (1, .005, .5, "18"), (0, .02, .5, "18")], ON),
    }
    at = list(range(20, int(L.HOLD * 1000) - 50, 400))
    for name, (pts, lp) in cases.items():
        xml = with_points(pts, lp)
        for side, ours in (("omni", False), ("ours", True)):
            y = L.trace(L.render(xml, ours), "pitch")
            print(f"{name:22s} {side}  " + " ".join(f"{y[i]:+4.0f}" for i in at), flush=True)


if __name__ == "__main__":
    curve([float(v) for v in sys.argv[2:]]) if sys.argv[1] == "curve" else run()
