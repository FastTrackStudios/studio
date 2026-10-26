#!/usr/bin/env python3
"""What each Omnisphere effect actually does at the settings patches use.

For every unit named (or every unit in the factory library), take its most
common parameter set across the patches, put it in the init part's first
Common FX slot, render a held note through real Omnisphere (daw's
`omni_render`) and compare against the same render with the slot empty:
level, 1/3-octave spectral change (RMS and tilt), stereo width, movement
(how much the held note's level swings) and tail after the key-up. A unit
that changes little here needs a gain, not a model.

usage: fxdelta.py <patches_dir> [unit ...]   (prints a TSV, biggest change first)
env: NOTE (48), OMNI_REFS (render cache)
"""
import collections, hashlib, os, re, struct, subprocess, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../../.."))
OMNI = os.path.join(REPO, "target/release/examples/omni_render")
REFS = os.environ.get("OMNI_REFS", "/tmp/omni-refs")
NOTE = os.environ.get("NOTE", "48")
SR, HOLD, TAIL = 48000, 2.0, 1.5
EDGES = 50 * 2 ** (np.arange(0, 28) / 3)


def hexf(v):
    return struct.pack(">f", v).hex()


def corpus_params(root):
    """unit → Counter of P0..P14 hex tuples, over every factory patch."""
    out = collections.defaultdict(collections.Counter)
    for dp, _, fs in os.walk(root):
        for f in fs:
            if not f.endswith(".prt_omn"):
                continue
            s = open(os.path.join(dp, f), encoding="latin-1").read()
            for m in re.finditer(r"<EFFMODULE\b[^>]*>", s):
                t = m.group(0)
                typ = re.search(r'Type="([^"]*)"', t).group(1)
                if typ in ("No Effect", "") or 'Active="0"' in t:
                    continue
                ps = tuple(re.search(r'\sP%d="([^"]*)"' % i, t).group(1) for i in range(15))
                out[typ][ps] += 1
    return out


def with_fx(xml, typ, ps, slot=16):
    ms = [m.start() for m in re.finditer(r"<EFFMODULE\b", xml)]
    a = ms[slot]
    b = xml.index(">", a)
    el = f'<EFFMODULE Type="{typ}" ' + " ".join(f'P{i}="{p}" ' for i, p in enumerate(ps)) + ' Active="3f800000"  MixLock="0" '
    return xml[:a] + el + xml[b:]


def render(xml):
    key = hashlib.sha1((xml + NOTE).encode()).hexdigest()[:16]
    out = os.path.join(REFS, f"fx_{key}.wav")
    if not os.path.exists(out):
        src = os.path.join(REFS, f"fx_{key}.prt_omn")
        open(src, "w", encoding="latin-1").write(xml)
        subprocess.run([OMNI, src, out, "--note", NOTE, "--hold", str(HOLD), "--tail", str(TAIL)],
                       capture_output=True, timeout=300, check=True)
    d = open(out, "rb").read()
    i = 12
    while i < len(d):
        cid, sz = d[i:i + 4], struct.unpack("<I", d[i + 4:i + 8])[0]
        if cid == b"data":
            return np.frombuffer(d[i + 8:i + 8 + sz], "<f4").reshape(-1, 2)
        i += 8 + sz + (sz & 1)


def bands(m):
    seg = m[int(0.5 * SR):int(HOLD * SR)]
    sp = np.abs(np.fft.rfft(seg * np.hanning(len(seg)))) ** 2
    f = np.fft.rfftfreq(len(seg), 1 / SR)
    return np.array([10 * np.log10(sp[(f >= lo) & (f < hi)].sum() + 1e-20) for lo, hi in zip(EDGES[:-1], EDGES[1:])])


def measure(x):
    m = x.mean(1)
    held = m[int(0.5 * SR):int(HOLD * SR)]
    e = np.array([np.mean(held[k:k + 2400] ** 2) for k in range(0, len(held) - 2400, 2400)]) + 1e-20
    tail = m[int((HOLD + 0.3) * SR):]
    side = np.mean((x[:, 0] - x[:, 1]) ** 2) / max(np.mean((x[:, 0] + x[:, 1]) ** 2), 1e-20)
    return dict(
        level=10 * np.log10(np.mean(held ** 2) + 1e-20),
        bands=bands(m),
        swing=float(np.ptp(10 * np.log10(e))),
        side=10 * np.log10(side + 1e-12),
        tail=10 * np.log10(np.mean(tail ** 2) + 1e-20),
    )


def main():
    root, units = sys.argv[1], sys.argv[2:]
    os.makedirs(REFS, exist_ok=True)
    params = corpus_params(root)
    base = open(os.path.join(HERE, "init_part.prt_omn"), encoding="latin-1").read()
    dry = measure(render(base))
    rows = []
    for typ in units or sorted(params, key=lambda t: -sum(params[t].values())):
        if typ not in params:
            print(f"# {typ}: not in the library", file=sys.stderr)
            continue
        ps, n = params[typ].most_common(1)[0]
        try:
            wet = measure(render(with_fx(base, typ, ps)))
        except subprocess.SubprocessError as e:
            print(f"# {typ}: render failed ({e})", file=sys.stderr)
            continue
        d = wet["bands"] - dry["bands"] - (wet["level"] - dry["level"])
        ok = dry["bands"] > dry["bands"].max() - 40  # dry-vs-dry noise 0.2 dB here
        spec = float(np.sqrt(np.mean(d[ok] ** 2)))
        tilt = float(np.polyfit(np.log2(EDGES[:-1][ok]), d[ok], 1)[0])
        rows.append((spec + abs(wet["level"] - dry["level"]), typ, sum(params[typ].values()), n,
                     wet["level"] - dry["level"], spec, tilt, wet["side"] - dry["side"],
                     wet["swing"] - dry["swing"], wet["tail"] - dry["tail"]))
        print(f"# {typ}", file=sys.stderr, flush=True)
    rows.sort(reverse=True)
    print("unit\tslots\tsame_set\tlevel_db\tspectrum_rms_db\ttilt_db_per_oct\twidth_db\tswing_db\ttail_db")
    for r in rows:
        print("\t".join([r[1], str(r[2]), str(r[3])] + [f"{v:+.1f}" for v in r[4:]]))


if __name__ == "__main__":
    main()
