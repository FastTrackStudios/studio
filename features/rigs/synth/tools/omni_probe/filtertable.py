#!/usr/bin/env python3
"""The layer filter alone, at the settings patches use.

For each filter type (`FILTER NameStr`) in order of use, take its most
common full `FILTER` settings across the factory patches, put them on the
init part's layer (a held saw; envelope depth zeroed unless `ENV=1`, so the
static filter is what is measured), render through real Omnisphere and
through Signal and print the level and held-spectrum gaps.

usage: filtertable.py <patches_dir> [--types 20] [--sets 2] [--note 48]
"""
import collections, hashlib, os, re, subprocess, sys

os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")  # as fxprobe.py: without it Signal falls back to generated waves

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../../.."))
OMNI = os.path.join(REPO, "target/release/examples/omni_render")
OURS = os.path.join(REPO, "target/release/examples/render_patch")
REFS = os.environ.get("OMNI_REFS", "/tmp/omni-refs")
LINE = re.compile(r"level\s+([-+\d.]+) dB.*held spectrum rms\s+([\d.]+) dB")


def arg(flag, default):
    a = sys.argv
    return type(default)(a[a.index(flag) + 1]) if flag in a else default


def filter_sets(root):
    by_type = collections.defaultdict(collections.Counter)
    for dp, _, fs in os.walk(root):
        for f in fs:
            if not f.endswith(".prt_omn"):
                continue
            s = open(os.path.join(dp, f), encoding="latin-1").read()
            for m in re.finditer(r"<FILTER\b[^>]*>", s):
                t = m.group(0)
                if re.search(r'\bact="0"', t):
                    continue
                name = re.search(r'NameStr="([^"]*)"', t)
                by_type[name.group(1) if name else "?"][t] += 1
    return by_type


def main():
    root = sys.argv[1]
    ntypes, nsets, note = arg("--types", 20), arg("--sets", 2), str(arg("--note", 48))
    base = open(os.path.join(HERE, "init_part.prt_omn"), encoding="latin-1").read()
    first = re.search(r"<FILTER\b[^>]*>", base)
    os.makedirs(REFS, exist_ok=True)
    env = dict(os.environ, RUST_LOG="error")
    timing = ["--note", note, "--vel", "100", "--hold", "2.0", "--tail", "0.5"]
    sets = filter_sets(root)
    for name in sorted(sets, key=lambda k: -sum(sets[k].values()))[:ntypes]:
        for tag, n in sets[name].most_common(nsets):
            if not os.environ.get("ENV"):
                tag = re.sub(r'\benvdpth="[^"]*"', 'envdpth="0"', tag)
            xml = base[:first.start()] + tag + base[first.end():]
            key = hashlib.sha1((xml + note).encode()).hexdigest()[:16]
            src, ref = os.path.join(REFS, f"flt_{key}.prt_omn"), os.path.join(REFS, f"flt_{key}.wav")
            ours = f"/tmp/filtertable_{key}.wav"
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
            freq = re.search(r'\bfreq="([^"]*)"', tag).group(1)
            res = re.search(r'\bres="([^"]*)"', tag).group(1)
            print(f"{sum(sets[name].values()):6d} {n:5d}  {name:26s} freq {freq:8s} res {res:8s} "
                  + (f"{float(m.group(2)):5.1f} dB held {float(m.group(1)):+5.1f} dB level  {src}" if m else "failed"),
                  flush=True)


if __name__ == "__main__":
    main()
