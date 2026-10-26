#!/usr/bin/env python3
"""How far each Omnisphere effect, as Signal realizes it, is from the real one.

For every unit (or those named), take its most common parameter set in the
factory patches, put it in the init part's first Common FX slot and render a
held note plus its tail through real Omnisphere and through Signal. Prints
our level gap and held-spectrum gap (`wavcmp.py`) and the tail-energy gap
after the key-up (delays and reverbs live there), ranked by
`slots × (spectrum + |level| + |tail|/2)` — the order to fix them in.

usage: fxgap.py <patches_dir> [unit ...]   env: NOTE (48), OMNI_REFS
"""
import hashlib, os, re, struct, subprocess, sys
import numpy as np

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fxdelta as F  # noqa: E402

OURS = os.path.join(F.REPO, "target/release/examples/render_patch")
LINE = re.compile(r"level\s+([-+\d.]+) dB.*held spectrum rms\s+([\d.]+) dB")
HOLD, TAIL = "2.0", "2.0"


def render(xml, ours):
    key = hashlib.sha1((xml + F.NOTE).encode()).hexdigest()[:16]
    src = os.path.join(F.REFS, f"fg_{key}.prt_omn")
    out = f"/tmp/fg_{key}_s.wav" if ours else os.path.join(F.REFS, f"fg_{key}.wav")
    open(src, "w", encoding="latin-1").write(xml)
    if ours or not os.path.exists(out):
        env = dict(os.environ, RUST_LOG="error", FTS_PRELOAD_PROFILE="full")
        subprocess.run([OURS if ours else F.OMNI, src, out, "--note", F.NOTE, "--hold", HOLD, "--tail", TAIL],
                       env=env, capture_output=True, timeout=300, check=True)
    return out


def tail_db(path):
    d = open(path, "rb").read()
    i = 12
    while i < len(d):
        cid, sz = d[i:i + 4], struct.unpack("<I", d[i + 4:i + 8])[0]
        if cid == b"data":
            x = np.frombuffer(d[i + 8:i + 8 + sz], "<f4").reshape(-1, 2).mean(1)
            t = x[int((float(HOLD) + 0.1) * F.SR):]
            return 10 * np.log10(np.mean(t ** 2) + 1e-20)
        i += 8 + sz + (sz & 1)


def main():
    root, units = sys.argv[1], sys.argv[2:]
    os.makedirs(F.REFS, exist_ok=True)
    params = F.corpus_params(root)
    init = open(os.path.join(F.HERE, "init_part.prt_omn"), encoding="latin-1").read()
    rows = []
    for typ in units or sorted(params, key=lambda t: -sum(params[t].values())):
        if typ not in params:
            continue
        ps, _ = params[typ].most_common(1)[0]
        xml = F.with_fx(init, typ, ps)
        try:
            ref, ours = render(xml, False), render(xml, True)
        except subprocess.SubprocessError:
            print(f"# {typ}: render failed", file=sys.stderr)
            continue
        out = subprocess.run([sys.executable, os.path.join(F.HERE, "wavcmp.py"), ref, ours],
                             env=dict(os.environ, HOLD=HOLD, BANDS="0"), capture_output=True, text=True).stdout
        m = LINE.search(out)
        if not m:
            continue
        lvl, spec = float(m.group(1)), float(m.group(2))
        tail = tail_db(ours) - tail_db(ref)
        tail = max(min(tail, 60.0), -60.0)
        slots = sum(params[typ].values())
        rows.append((slots * (spec + abs(lvl) + abs(tail) / 2), typ, slots, lvl, spec, tail))
        print(f"# {typ}: level {lvl:+.1f} spectrum {spec:.1f} tail {tail:+.1f}", file=sys.stderr, flush=True)
    rows.sort(reverse=True)
    print("unit\tslots\tlevel_db\tspectrum_db\ttail_db")
    for _, typ, slots, lvl, spec, tail in rows:
        print(f"{typ}\t{slots}\t{lvl:+.1f}\t{spec:.1f}\t{tail:+.1f}")


if __name__ == "__main__":
    main()
