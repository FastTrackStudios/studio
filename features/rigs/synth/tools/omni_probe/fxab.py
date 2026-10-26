#!/usr/bin/env python3
"""One Omnisphere effect parameter swept through both engines: the wet
level (dB re the dry), the held-spectrum gap and the tail gap per step.

usage: fxab.py "<Unit>" "P0=v,P1=v,..." Pn v1,v2,...   env: NOTE (48), OMNI_REFS
"""
import os, subprocess, sys

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves
os.environ.setdefault("NOTE", "48")

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fxdelta as F  # noqa: E402
import fxgap as G  # noqa: E402


def main():
    unit, base, key, values = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
    ps = [0.0] * 15
    for kv in base.split(","):
        k, v = kv.split("=")
        ps[int(k.lstrip("P"))] = float(v)
    init = open(os.path.join(F.HERE, "init_part.prt_omn"), encoding="latin-1").read()
    os.makedirs(F.REFS, exist_ok=True)
    dry = F.measure(F.render(init))["level"]
    print(f"{unit}  dry {dry:.1f} dB")
    for v in (float(x) for x in values.split(",")):
        p = list(ps)
        p[int(key.lstrip("P"))] = v
        xml = F.with_fx(init, unit, [F.hexf(x) for x in p])
        ref, ours = G.render(xml, False), G.render(xml, True)
        out = subprocess.run([sys.executable, os.path.join(F.HERE, "wavcmp.py"), ref, ours],
                             env=dict(os.environ, HOLD=G.HOLD, BANDS="0"), capture_output=True, text=True).stdout
        m = G.LINE.search(out)
        lo = F.measure(F.render(xml))["level"]
        tail = G.tail_db(ours) - G.tail_db(ref)
        print(f"{key}={v:<6} omni {lo - dry:+6.1f} dB  ours−omni {float(m.group(1)):+5.1f} dB  "
              f"spectrum {float(m.group(2)):5.1f} dB  tail {tail:+6.1f} dB", flush=True)


if __name__ == "__main__":
    main()
