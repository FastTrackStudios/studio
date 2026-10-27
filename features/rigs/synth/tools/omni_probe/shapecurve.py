#!/usr/bin/env python3
"""The Omnisphere Waveshaper's transfer curve, read off a sine.

Renders the init part playing a sine (analog drift off) through real
Omnisphere with the layer's Waveshaper off and on, aligns the two, and bins
output against input: the curve y = F(x), and the spread within each bin
(near zero → a static, memoryless curve). Prints the curve on a grid of
input levels.

usage: shapecurve.py type dpth gain [bins]   env: NOTE (36), OMNI_REFS
"""
import hashlib, os, re, struct, subprocess, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../../.."))
OMNI = os.path.join(REPO, "target/release/examples/omni_render")
REFS = os.environ.get("OMNI_REFS", "/Volumes/dev-drive/AudioHaven/Sampled/Synth/Omnisphere-Measurements/refs")
NOTE = os.environ.get("NOTE", "36")
SINE = "~BundleArchives/1 - Classic Waveforms/Triangle + Sine/Moog Modular Sine+Tri/Moog Modular Sine.stmwf"
SR = 48000


def hexf(v):
    return struct.pack(">f", v).hex()


def edit(xml, tag, attrs):
    m = re.search(r"<%s\b[^>]*>" % tag, xml)
    t = m.group(0)
    for k, v in attrs.items():
        t = re.sub(r'(\s%s=)"[^"]*"' % k, r'\1"%s"' % v, t)
    return xml[:m.start()] + t + xml[m.end():]


def render(xml):
    key = hashlib.sha1((xml + NOTE).encode()).hexdigest()[:16]
    src, out = os.path.join(REFS, f"sc_{key}.prt_omn"), os.path.join(REFS, f"sc_{key}.wav")
    if not os.path.exists(out):
        open(src, "w", encoding="latin-1").write(xml)
        subprocess.run([OMNI, src, out, "--note", NOTE, "--hold", "1.5", "--tail", "0.2"],
                       capture_output=True, timeout=300, check=True)
    d = open(out, "rb").read()
    i = 12
    while i < len(d):
        cid, sz = d[i:i + 4], struct.unpack("<I", d[i + 4:i + 8])[0]
        if cid == b"data":
            return np.frombuffer(d[i + 8:i + 8 + sz], "<f4").reshape(-1, 2)[:, 0]
        i += 8 + sz + (sz & 1)


def base_xml():
    xml = open(os.path.join(HERE, "init_part.prt_omn"), encoding="latin-1").read()
    xml = re.sub(r'<WAVES wf0="[^"]*" wf1="[^"]*"', f'<WAVES wf0="{SINE}" wf1="{SINE}"', xml, count=1)
    return edit(xml, "OSC", {"sana": "0", "odrft": "0"})


def curve(kind, dpth, gain, bins=24):
    """(x grid, y = F(x), spread) for one setting."""
    xml = base_xml()
    x = render(xml)
    y = render(edit(xml, "WAVESHAPER", {"act": hexf(1.0), "type": hexf(kind), "dpth": hexf(dpth),
                                         "gain": hexf(gain), "mix": hexf(1.0), "bc": "0", "srrdc": "0"}))
    seg = slice(int(0.4 * SR), int(1.4 * SR))
    xs, ys = x[seg], y[seg]
    # Align (a shaper may add latency).
    lag = int(np.argmax([np.dot(xs[:-64], np.roll(ys, -k)[:-64]) for k in range(0, 64)]))
    ys = np.roll(ys, -lag)
    peak = np.max(np.abs(xs))
    edges = np.linspace(-peak, peak, bins + 1)
    idx = np.clip(np.digitize(xs, edges) - 1, 0, bins - 1)
    grid = 0.5 * (edges[:-1] + edges[1:])
    fy = np.array([ys[idx == b].mean() if np.any(idx == b) else np.nan for b in range(bins)])
    sd = np.array([ys[idx == b].std() if np.any(idx == b) else np.nan for b in range(bins)])
    return grid, fy, sd, lag, peak


def main():
    kind, dpth, gain = (float(v) for v in sys.argv[1:4])
    bins = int(sys.argv[4]) if len(sys.argv) > 4 else 24
    os.makedirs(REFS, exist_ok=True)
    grid, fy, sd, lag, peak = curve(kind, dpth, gain, bins)
    print(f"type {kind} dpth {dpth} gain {gain}: input peak {peak:.3f}, lag {lag}, "
          f"max spread {np.nanmax(sd):.4f} (curve span {np.nanmax(fy) - np.nanmin(fy):.3f})")
    for g, v, s in zip(grid, fy, sd):
        print(f"  x {g:+.3f}  y {v:+.4f}  ±{s:.4f}")


if __name__ == "__main__":
    main()
