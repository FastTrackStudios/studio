#!/usr/bin/env python3
"""Sweep one attribute of a layer-A element of the init part, through real
Omnisphere and through Signal: what the plugin does as it moves (level and
spectral tilt against the base setting) and how far ours is from it (level
and held-spectrum gap, from `wavcmp.py`).

usage: vsweep.py TAG "attr=v,attr=v" attr v1,v2,...   (values as floats)
e.g.   vsweep.py WAVESHAPER "act=1,dpth=0.3" gain 0,0.25,0.5,0.75,1
env: NOTE (48), OMNI_REFS
"""
import hashlib, os, re, struct, subprocess, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../../.."))
OMNI = os.path.join(REPO, "target/release/examples/omni_render")
OURS = os.path.join(REPO, "target/release/examples/render_patch")
REFS = os.environ.get("OMNI_REFS", "/tmp/omni-refs")
NOTE = os.environ.get("NOTE", "48")
LINE = re.compile(r"level\s+([-+\d.]+) dB.*held spectrum rms\s+([\d.]+) dB")
SR = 48000


def hexf(v):
    return struct.pack(">f", v).hex()


def edit(xml, tag, attrs):
    m = re.search(r"<%s\b[^>]*>" % tag, xml)
    t = m.group(0)
    for k, v in attrs.items():
        t2, n = re.subn(r'(\s%s=)"[^"]*"' % k, r'\1"%s"' % hexf(v), t)
        if n == 0:
            t2 = t[:-1].rstrip() + f' {k}="{hexf(v)}" >'
        t = t2
    return xml[:m.start()] + t + xml[m.end():]


def read(p):
    d = open(p, "rb").read()
    i = 12
    while i < len(d):
        cid, sz = d[i:i + 4], struct.unpack("<I", d[i + 4:i + 8])[0]
        if cid == b"data":
            return np.frombuffer(d[i + 8:i + 8 + sz], "<f4").reshape(-1, 2).mean(1)
        i += 8 + sz + (sz & 1)


def render(xml, ours):
    key = hashlib.sha1((xml + NOTE).encode()).hexdigest()[:16]
    src = os.path.join(REFS, f"vs_{key}.prt_omn")
    out = f"/tmp/vs_{key}_s.wav" if ours else os.path.join(REFS, f"vs_{key}.wav")
    open(src, "w", encoding="latin-1").write(xml)
    if ours or not os.path.exists(out):
        env = dict(os.environ, RUST_LOG="error", FTS_PRELOAD_PROFILE="full")
        subprocess.run([OURS if ours else OMNI, src, out, "--note", NOTE, "--hold", "2.0", "--tail", "0.3"],
                       env=env, capture_output=True, timeout=300, check=True)
    return out


def level_tilt(path, ref):
    x, r = read(path), read(ref)
    seg = slice(int(0.5 * SR), int(1.9 * SR))
    lvl = 10 * np.log10(np.mean(x[seg] ** 2) + 1e-20) - 10 * np.log10(np.mean(r[seg] ** 2) + 1e-20)
    def cent(y):
        s = np.abs(np.fft.rfft(y[seg] * np.hanning(len(y[seg])))) ** 2
        f = np.fft.rfftfreq(len(y[seg]), 1 / SR)
        return np.sum(f * s) / max(np.sum(s), 1e-20)
    return lvl, cent(x)


def main():
    tag, base_s, attr, values = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
    base = {k: float(v) for k, v in (kv.split("=") for kv in base_s.split(",") if kv)}
    init = open(os.path.join(HERE, "init_part.prt_omn"), encoding="latin-1").read()
    os.makedirs(REFS, exist_ok=True)
    off = render(init, False)
    print(f"{tag} {base}  sweeping {attr}  (omni: level / centroid vs the init part; gap: ours − omni)")
    for v in (float(x) for x in values.split(",")):
        xml = edit(init, tag, {**base, attr: v})
        ref, ours = render(xml, False), render(xml, True)
        lvl, cent = level_tilt(ref, off)
        out = subprocess.run([sys.executable, os.path.join(HERE, "wavcmp.py"), ref, ours],
                             env=dict(os.environ, HOLD="2.0", BANDS="0"), capture_output=True, text=True).stdout
        m = LINE.search(out)
        gap = f"gap level {float(m.group(1)):+5.1f} dB  spectrum {float(m.group(2)):5.1f} dB" if m else "gap ?"
        print(f"  {attr}={v:<6.3f} omni {lvl:+6.1f} dB  centroid {cent:6.0f} Hz   {gap}", flush=True)


if __name__ == "__main__":
    main()
