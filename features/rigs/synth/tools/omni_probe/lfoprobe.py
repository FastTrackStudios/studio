#!/usr/bin/env python3
"""An Omnisphere LFO, read off a pure sine, against ours.

Builds a test preset from the init part — a sine oscillator (no analog
drift), one mod-matrix row LFO1 → the target, LFO1 set as asked — renders a
held C4 through real Omnisphere and through Signal, and recovers the
modulation itself: the instantaneous pitch (cents) or amplitude (dB) over
time. Prints, per side, the rate (Hz), depth (peak-to-peak), the value at
note-on, the waveform over one cycle (10 points, normalized) and the depth
in the first vs the last second (a fade-in shows as a ratio below 1).

usage: lfoprobe.py pitch|amp|filter "rate=0.4,type=0,swing=1,..." [lo=0,hi=0.5,defV=1] [label]
  LFO attributes: rate type swing phase envdpth resettr sync unidir bpolar center
env: OMNI_REFS, HOLD (3.0), NOTE (60), SOURCE (LFO1; any matrix source),
     PLOT=1 prints both traces every 20 ms
"""
import hashlib, os, re, struct, subprocess, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shapecurve as S  # noqa: E402

OURS = os.path.join(S.REPO, "target/release/examples/render_patch")
SR = 48000
HOLD = float(os.environ.get("HOLD", "3.0"))
NOTE = os.environ.get("NOTE", "60")
TARGET = {"pitch": "A tuneFine", "amp": "A atrm", "filter": "A freq"}


def preset(target, lfo, route):
    if target == "filter":
        # The init part's saw through an engaged plain 12 dB low-pass at a
        # mid setting, its own envelope off: the LFO alone moves the corner.
        xml = open(os.path.join(S.HERE, "init_part.prt_omn"), encoding="latin-1").read()
        xml = S.edit(xml, "OSC", {"sana": "0", "odrft": "0"})
        xml = S.edit(xml, "FILTER", {"act": S.hexf(1.0), "type1": S.hexf(0.22),
                                     "freq": S.hexf(float(os.environ.get("CUTOFF", "0.35"))),
                                     "envdpth": "0", "res": "0", "key": "0"})
    elif os.environ.get("BASE"):
        # Any part (e.g. a sample layer), drift off: LFO1 drives its layer A.
        xml = open(os.environ["BASE"], encoding="latin-1").read()
        xml = S.edit(xml, "OSC", {"sana": "0", "odrft": "0"})
    else:
        xml = S.base_xml()
    a = xml.index("<LFO_SET")
    m = re.search(r"<LFO\b[^>]*>", xml[a:])
    tag = m.group(0)
    for k, v in lfo.items():
        tag, n = re.subn(r'(\s%s=)"[^"]*"' % k, r'\1"%s"' % S.hexf(v), tag)
        assert n == 1, k
    xml = xml[:a + m.start()] + tag + xml[a + m.end():]
    mm = re.search(r"<MOD_MATRIX\b[^>]*>", xml)
    t = mm.group(0)
    src = os.environ.get("SOURCE", "LFO1")
    t = re.sub(r'(\ssource0=)"[^"]*"', rf'\1"{src}"', t, count=1)
    t = re.sub(r'(\starget0=)"[^"]*"', rf'\1"{TARGET[target]}"', t, count=1)
    t = re.sub(r'(\slo0=)"[^"]*"', r'\1"%s"' % S.hexf(route.get("lo", 0.0)), t)
    t = re.sub(r'(\shi0=)"[^"]*"', r'\1"%s"' % S.hexf(route.get("hi", 0.5)), t)
    if "defV" in route:
        t = re.sub(r'(\sdefV0=)"[^"]*"', r'\1"%s"' % S.hexf(route["defV"]), t)
    return xml[:mm.start()] + t + xml[mm.end():]


def read(path):
    d = open(path, "rb").read()
    i = 12
    while i < len(d):
        cid, sz = d[i:i + 4], struct.unpack("<I", d[i + 4:i + 8])[0]
        if cid == b"data":
            return np.frombuffer(d[i + 8:i + 8 + sz], "<f4").reshape(-1, 2)[:, 0].astype(np.float64)
        i += 8 + sz + (sz & 1)


def render(xml, ours):
    key = hashlib.sha1((xml + NOTE + str(HOLD)).encode()).hexdigest()[:16]
    src = os.path.join(S.REFS, f"lfo_{key}.prt_omn")
    out = f"/Volumes/dev-drive/scratch/omni-probe/lfo_{key}_s.wav" if ours else os.path.join(S.REFS, f"lfo_{key}.wav")
    open(src, "w", encoding="latin-1").write(xml)
    if ours or not os.path.exists(out):
        env = dict(os.environ, RUST_LOG="error")
        subprocess.run([OURS if ours else S.OMNI, src, out, "--note", NOTE, "--hold", str(HOLD), "--tail", "0.1"],
                       env=env, capture_output=True, timeout=300, check=True)
    return read(out)


def analytic(x, band=None):
    """The analytic signal, keeping only `band` (Hz) when given — the sine
    table carries a few harmonics, which would wreck the phase."""
    n = len(x)
    X = np.fft.fft(x)
    f = np.fft.fftfreq(n, 1 / SR)
    h = np.where(f > 0, 2.0, 0.0)
    h[0] = 1.0
    if band:
        h *= (f >= band[0]) & (f <= band[1])
    return np.fft.ifft(X * h)


def trace(x, kind):
    """The modulation every 1 ms over the held note (the filter's as the
    spectral centroid in octaves re 1 kHz, every 1 ms from 20 ms frames)."""
    if kind == "filter":
        n, hop = 960, SR // 1000
        w = np.hanning(n)
        f = np.fft.rfftfreq(n, 1 / SR)
        out = []
        for k in range(0, int(HOLD * SR) - n, hop):
            sp = np.abs(np.fft.rfft(x[k:k + n] * w)) ** 2
            out.append(np.log2(max(np.sum(f * sp) / max(np.sum(sp), 1e-20), 1.0) / 1000))
        return np.array(out)
    f0 = 440 * 2 ** ((float(NOTE) - 69) / 12)
    z = analytic(x[: int(HOLD * SR)], (0.5 * f0, 1.5 * f0))
    hop = SR // 1000
    if kind == "pitch":
        ph = np.unwrap(np.angle(z))
        f = np.diff(ph) * SR / (2 * np.pi)
        k = np.convolve(f, np.ones(96) / 96, mode="same")  # 2 ms
        y = 1200 * np.log2(np.maximum(k, 1.0) / f0)
    else:
        a = np.abs(z)
        a = np.convolve(a, np.ones(96) / 96, mode="same")
        y = 20 * np.log10(np.maximum(a, 1e-9))
    return y[::hop]


def describe(y):
    t0 = 50  # skip the first 50 ms (attack, filter settling)
    s = y[t0:-20]
    s = s - np.median(s)
    sp = np.abs(np.fft.rfft(s * np.hanning(len(s)), 16 * len(s)))
    fr = np.fft.rfftfreq(16 * len(s), 1e-3)
    ok = fr > 0.05
    rate = fr[ok][np.argmax(sp[ok])]
    p2p = np.percentile(s, 99) - np.percentile(s, 1)
    first, last = s[:1000], s[-1000:]
    fade = (np.ptp(first) + 1e-9) / (np.ptp(last) + 1e-9)
    # One cycle, from the trace's last full period, 10 points.
    per = max(int(round(1000 / max(rate, 0.05))), 2)
    cyc = s[-per:] if per < len(s) else s
    ix = np.linspace(0, len(cyc) - 1, 10).astype(int)
    span = (cyc.max() - cyc.min()) or 1.0
    shape = (cyc[ix] - cyc.min()) / span
    return rate, p2p, y[t0] - np.median(y[t0:-20]), fade, shape


def main():
    kind = sys.argv[1]
    lfo = {k: float(v) for k, v in (kv.split("=") for kv in sys.argv[2].split(",") if kv)}
    route = {k: float(v) for k, v in (kv.split("=") for kv in sys.argv[3].split(",") if kv)} if len(sys.argv) > 3 and "=" in sys.argv[3] else {}
    label = sys.argv[-1] if len(sys.argv) > 3 and "=" not in sys.argv[-1] else sys.argv[2]
    os.makedirs(S.REFS, exist_ok=True)
    xml = preset(kind, lfo, route)
    unit = {"pitch": "c", "amp": "dB", "filter": "oct"}[kind]
    ys = {}
    for side, ours in (("omni", False), ("ours", True)):
        y = trace(render(xml, ours), kind)
        ys[side] = y
        rate, p2p, start, fade, shape = describe(y)
        print(f"{label[:34]:34s} {side}  rate {rate:6.3f} Hz  depth {p2p:7.1f} {unit} p-p  start {start:+6.1f}  "
              f"fade {fade:4.2f}  shape " + " ".join(f"{v:.2f}" for v in shape), flush=True)
    if os.environ.get("PLOT"):
        o, s = ys["omni"], ys["ours"]
        for i in range(0, min(len(o), len(s)), 20):
            print(f"  {i:5d} ms  omni {o[i]:+7.1f}  ours {s[i]:+7.1f}")


if __name__ == "__main__":
    main()
