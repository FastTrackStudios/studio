#!/usr/bin/env python3
"""|H| of one Omnisphere effect, per harmonic, real Omnisphere against ours.

Renders the init part (a held saw) dry and with the unit in the first Common
FX slot, through Omnisphere (`omni_render`) and through Signal
(`render_patch`, which realizes the unit via `omni_import::effects`), and
prints |H| = wet / dry at each harmonic of the held note for both, plus the
RMS dB gap over the harmonics within 40 dB of the dry peak. That gap is the
number a unit's model has to drive down.

usage: fxh.py "<Unit>" P0=v,P1=v,... [P0=v,...]   (one line per set)
env: NOTE (36), SHOW (harmonic list, default a log spread), OURS=0 skips ours
"""
import os, subprocess, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("NOTE", "36")
import fxdelta as F  # noqa: E402

OURS_EXE = os.path.join(F.REPO, "target/release/examples/render_patch")


def ours(xml):
    src = "/tmp/fxh_ours.prt_omn"
    open(src, "w", encoding="latin-1").write(xml)
    out = "/tmp/fxh_ours.wav"
    env = dict(os.environ, RUST_LOG="error", FTS_PRELOAD_PROFILE="full")
    subprocess.run([OURS_EXE, src, out, "--note", F.NOTE, "--hold", str(F.HOLD), "--tail", str(F.TAIL)],
                   env=env, capture_output=True, timeout=300, check=True)
    d = open(out, "rb").read()
    import struct
    i = 12
    while i < len(d):
        cid, sz = d[i:i + 4], struct.unpack("<I", d[i + 4:i + 8])[0]
        if cid == b"data":
            return np.frombuffer(d[i + 8:i + 8 + sz], "<f4").reshape(-1, 2)
        i += 8 + sz + (sz & 1)


def harmonics(x):
    """Magnitude (dB) at each harmonic of the held note, by peak-picking."""
    m = x.mean(1)[int(0.5 * F.SR):int(F.HOLD * F.SR)]
    n = len(m)
    sp = np.abs(np.fft.rfft(m * np.hanning(n), 4 * n))
    f0 = 440 * 2 ** ((float(F.NOTE) - 69) / 12)
    df = F.SR / (4 * n)
    hs = np.arange(1, int(20000 / f0))
    out = []
    for h in hs:
        lo, hi = int(h * f0 * 0.98 / df), int(h * f0 * 1.02 / df) + 1
        out.append(20 * np.log10(sp[lo:hi].max() + 1e-12))
    return hs * f0, np.array(out)


def parse(s):
    ps = [0.0] * 15
    for kv in s.split(","):
        if kv:
            k, v = kv.split("=")
            ps[int(k.lstrip("P"))] = float(v)
    return ps


def main():
    unit = sys.argv[1]
    init = open(os.path.join(F.HERE, "init_part.prt_omn"), encoding="latin-1").read()
    os.makedirs(F.REFS, exist_ok=True)
    f, dry_o = harmonics(F.render(init))
    want_ours = os.environ.get("OURS", "1") != "0"
    dry_s = harmonics(ours(init))[1] if want_ours else None
    ok = dry_o > dry_o.max() - 40
    show = [int(v) for v in os.environ.get("SHOW", "1,2,3,4,6,8,11,16,22,32,45,64,90,128,180,250").split(",") if int(v) <= len(f)]
    print(f"{unit}   harmonics of {f[0]:.1f} Hz; columns: |H| omni / ours (dB)")
    print("set".ljust(34) + " ".join(f"{f[h - 1]:>9.0f}" for h in show) + "   rms")
    for s in sys.argv[2:]:
        ps = parse(s)
        xml = F.with_fx(init, unit, [F.hexf(v) for v in ps])
        h_o = harmonics(F.render(xml))[1] - dry_o
        line_o = s[:33].ljust(34) + " ".join(f"{h_o[h - 1]:>9.1f}" for h in show)
        print(line_o)
        if want_ours:
            h_s = harmonics(ours(xml))[1] - dry_s
            gap = float(np.sqrt(np.mean((h_s - h_o)[ok] ** 2)))
            print("  ours".ljust(34) + " ".join(f"{h_s[h - 1]:>9.1f}" for h in show) + f"  {gap:5.1f}", flush=True)


if __name__ == "__main__":
    main()
