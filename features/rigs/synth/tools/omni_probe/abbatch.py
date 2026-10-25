#!/usr/bin/env python3
"""A/B a spread of Omnisphere patches: real Omnisphere (daw's `omni_render`)
against Signal (`render_patch`), same note and timing, scored by `wavcmp.py`.

usage: abbatch.py <coverage.tsv> <out_dir> [--per-lib 3] [--note 48] [--seed 1]

`coverage.tsv` is `patch_coverage --tsv`; only patches with no known gap are
drawn, `--per-lib` from each library folder, so a bad score is a fidelity
bug, not a missing feature. Writes `<out_dir>/ab.tsv` (worst first) and
prints a summary. Needs both examples built in release and numpy (run it
with `uv run --with numpy python abbatch.py …`); Signal reads its library
roots from the environment (`source .env`).
"""
import os, random, re, subprocess, sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../../.."))
OMNI = os.path.join(REPO, "target/release/examples/omni_render")
OURS = os.path.join(REPO, "target/release/examples/render_patch")
HOLD, TAIL = "2.0", "1.0"
LINE = re.compile(r"level\s+([-+\d.]+) dB.*held spectrum rms\s+([\d.]+) dB\s+attack\s+([\d.]+) dB")


def arg(flag, default):
    a = sys.argv
    return type(default)(a[a.index(flag) + 1]) if flag in a else default


def pick(tsv, per_lib, seed):
    libs = defaultdict(list)
    for line in open(tsv, encoding="utf-8"):
        path, _, gaps = line.rstrip("\n").partition("\t")
        if path == "patch" or gaps:
            continue
        m = re.search(r"/Patches/([^/]+/[^/]+)/", path)
        libs[m.group(1) if m else "?"].append(path)
    rng = random.Random(seed)
    return [(lib, p) for lib, ps in sorted(libs.items()) for p in rng.sample(ps, min(per_lib, len(ps)))]


def main():
    tsv, out = sys.argv[1], sys.argv[2]
    per_lib, note, seed = arg("--per-lib", 3), arg("--note", 48), arg("--seed", 1)
    os.makedirs(out, exist_ok=True)
    env = dict(os.environ, RUST_LOG="error")
    rows = []
    for i, (lib, patch) in enumerate(pick(tsv, per_lib, seed)):
        ref, ours = os.path.join(out, f"{i:03d}_omni.wav"), os.path.join(out, f"{i:03d}_ours.wav")
        timing = ["--note", str(note), "--vel", "100", "--hold", HOLD, "--tail", TAIL]
        try:
            subprocess.run([OMNI, patch, ref, *timing], env=env, capture_output=True, timeout=120, check=True)
            subprocess.run([OURS, patch, ours, *timing], env=env, capture_output=True, timeout=120, check=True)
            cmp = subprocess.run([sys.executable, os.path.join(HERE, "wavcmp.py"), ref, ours],
                                 env=dict(env, HOLD=HOLD, BANDS="0"), capture_output=True, text=True, timeout=60)
            m = LINE.search(cmp.stdout)
        except (subprocess.SubprocessError, OSError) as e:
            m, cmp = None, e
        if not m:
            rows.append((lib, patch, None, None, None, "render/compare failed"))
            continue
        level, held, attack = (float(g) for g in m.groups())
        rows.append((lib, patch, level, held, attack, ""))
        print(f"{i:3d} {held:5.1f} dB held  {level:+5.1f} dB level  {os.path.basename(patch)}", flush=True)

    ok = [r for r in rows if r[3] is not None]
    ok.sort(key=lambda r: -r[3])
    with open(os.path.join(out, "ab.tsv"), "w", encoding="utf-8") as f:
        f.write("library\tpatch\tlevel_db\theld_spectrum_db\tattack_db\tnote\n")
        for r in ok + [r for r in rows if r[3] is None]:
            f.write("\t".join("" if v is None else (f"{v:.2f}" if isinstance(v, float) else v) for v in r) + "\n")
    if ok:
        held = sorted(r[3] for r in ok)
        level = sorted(abs(r[2]) for r in ok)
        med = lambda xs: xs[len(xs) // 2]
        print(f"\n{len(ok)} compared, {len(rows) - len(ok)} failed")
        print(f"held spectrum rms: median {med(held):.1f} dB, <=3 dB {sum(h <= 3 for h in held)}, >8 dB {sum(h > 8 for h in held)}")
        print(f"|level|: median {med(level):.1f} dB, <=2 dB {sum(l <= 2 for l in level)}")


if __name__ == "__main__":
    main()
