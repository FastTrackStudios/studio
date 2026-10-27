#!/usr/bin/env python3
"""The wavetable oscillator alone, across the wavetables patches use.

Takes the init part (one synth layer, filter / waveshaper / FX off), swaps
in each of the most-used `WAVES wf0/wf1` pairs (and, with `SHAPE=1`, the
patch's Shape position), renders a held note through real Omnisphere and
through Signal and prints the level gap and held-spectrum gap per wavetable
— the oscillator's fidelity ranked by how many patches lean on it.

usage: osctable.py <patches_dir> [--top 40] [--note 48]
env: OMNI_REFS (reference cache)
"""
import collections, hashlib, os, re, subprocess, sys

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../../.."))
OMNI = os.path.join(REPO, "target/release/examples/omni_render")
OURS = os.path.join(REPO, "target/release/examples/render_patch")
REFS = os.environ.get("OMNI_REFS", "/Volumes/dev-drive/AudioHaven/Sampled/Synth/Omnisphere-Measurements/refs")
LINE = re.compile(r"level\s+([-+\d.]+) dB.*held spectrum rms\s+([\d.]+) dB")


def arg(flag, default):
    a = sys.argv
    return type(default)(a[a.index(flag) + 1]) if flag in a else default


def wave_pairs(root):
    pairs = collections.Counter()
    for dp, _, fs in os.walk(root):
        for f in fs:
            if not f.endswith(".prt_omn"):
                continue
            s = open(os.path.join(dp, f), encoding="latin-1").read()
            for m in re.finditer(r'<WAVES\s+wf0="([^"]+)"\s+wf1="([^"]*)"', s):
                pairs[(m.group(1), m.group(2))] += 1
    return pairs


def main():
    root = sys.argv[1]
    top, note = arg("--top", 40), str(arg("--note", 48))
    base = open(os.path.join(HERE, "init_part.prt_omn"), encoding="latin-1").read()
    os.makedirs(REFS, exist_ok=True)
    env = dict(os.environ, RUST_LOG="error")
    timing = ["--note", note, "--vel", "100", "--hold", "2.0", "--tail", "0.5"]
    rows = []
    for (w0, w1), n in wave_pairs(root).most_common(top):
        xml = re.sub(r'<WAVES wf0="[^"]*" wf1="[^"]*"', f'<WAVES wf0="{w0}" wf1="{w1}"', base, count=1)
        key = hashlib.sha1((xml + note).encode()).hexdigest()[:16]
        src, ref = os.path.join(REFS, f"osc_{key}.prt_omn"), os.path.join(REFS, f"osc_{key}.wav")
        ours = f"/Volumes/dev-drive/scratch/omni-probe/osctable_{key}.wav"
        open(src, "w", encoding="latin-1").write(xml)
        try:
            if not os.path.exists(ref):
                subprocess.run([OMNI, src, ref, *timing], env=env, capture_output=True, timeout=300, check=True)
            subprocess.run([OURS, src, ours, *timing], env=env, capture_output=True, timeout=300, check=True)
            out = subprocess.run([sys.executable, os.path.join(HERE, "wavcmp.py"), ref, ours],
                                 env=dict(env, HOLD="2.0", BANDS="0"), capture_output=True, text=True).stdout
            m = LINE.search(out)
        except subprocess.SubprocessError:
            m = None
        name = w0.split("/")[-1]
        if m:
            rows.append((float(m.group(2)), float(m.group(1)), n, name, src))
            print(f"{n:6d} {float(m.group(2)):5.1f} dB held {float(m.group(1)):+5.1f} dB level  {name}", flush=True)
        else:
            print(f"{n:6d}  failed  {name}", flush=True)
    if rows:
        held = sorted(r[0] for r in rows)
        w = sum(r[2] for r in rows)
        print(f"\n{len(rows)} wavetables, median held {held[len(held)//2]:.1f} dB, "
              f"use-weighted mean {sum(r[0]*r[2] for r in rows)/w:.1f} dB, <=2 dB {sum(h <= 2 for h in held)}")


if __name__ == "__main__":
    main()
