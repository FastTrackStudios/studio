#!/usr/bin/env python3
"""Fit Kontakt's random draws in a probe render, for render_css_live to replay.

    python3 fit_draws.py --tunes|--variants|--attacks|--rrs <render_css_live> <Section> <probe.mid> <kontakt.wav> <out.wav>

Kontakt draws some choices at random: a short's +-5 c detune (a key not
struck in the last 500 ms), a zone-2 legato body variant (2..4) and a loud
first note's attack velocity (an index into 5-7 / 8-9). They
cannot be reproduced, so they are fitted from the render and forced into
ours (render_css_live --tunes / --variants take them in the order they are
drawn).

Our engine is rendered once per candidate value with EVERY draw forced to
that value. A note drew where the voices our engine started differ between
the lowest and highest candidate (render_css_live --trace: a drawn detune
changes the voice's rate, a drawn variant its sample); each such note gets
the candidate that leaves the least residual against Kontakt between its
onset and the next note-on. Prints the fitted list and writes our render
with it to <out.wav>. No knowledge of which notes draw is needed.
"""
import os, re, struct, subprocess, sys, tempfile

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from null_test import SR, load  # noqa: E402

MAX_DRAWS = 400
REFINE_DB = -30.0


def note_ons(path):
    """Note-on times (s) of a single-track SMF written by gen_probe.py."""
    d = open(path, "rb").read()
    ppq = struct.unpack(">H", d[12:14])[0]
    p = 14 + 8
    end = len(d)
    t = 0
    tempo = 500000
    run = None
    out = []
    while p < end:
        v = 0
        while True:
            b = d[p]
            p += 1
            v = (v << 7) | (b & 0x7F)
            if b < 0x80:
                break
        t += v
        st = d[p]
        if st == 0xFF:
            ty, ln = d[p + 1], d[p + 2]
            if ty == 0x51:
                tempo = int.from_bytes(d[p + 3 : p + 6], "big")
            if ty == 0x2F:
                break
            p += 3 + ln
            continue
        if st & 0x80:
            run = st
            p += 1
        n = 1 if (run & 0xF0) in (0xC0, 0xD0) else 2
        if run & 0xF0 == 0x90 and d[p + 1] > 0:
            out.append(t * tempo / ppq / 1e6)
        p += n
    return sorted(out)


VOICE = re.compile(r"^\[v\d+\]\s+([\d.]+)s\s+(.*)$")


def render(binary, section, mid, flag, values, out, trace=False):
    """Our render with `values` forced; with `trace`, also the voices started
    as (time, description) in order."""
    args = [binary, mid, out, "--section", section, "--library", os.environ.get("CS_LIBRARY", "strings")]
    if values is not None:
        args += [flag, ",".join(str(v) for v in values)]
    if trace:
        args.append("--trace")
    r = subprocess.run(args, check=True, capture_output=True, text=True, timeout=900)
    voices = [(float(m.group(1)), m.group(2)) for m in map(VOICE.match, (r.stdout + r.stderr).splitlines()) if m]
    return load(out), voices


def energy(x):
    return float(np.sum(x * x))


def main():
    kind = sys.argv[1]
    binary, section, mid, kon, out_wav = sys.argv[2:7]
    flag, cands = {
        "--tunes": ("--tunes", range(-5, 6)),
        "--variants": ("--variants", (2, 3, 4)),
        "--attacks": ("--attacks", (0, 1, 2)),
        # random short round robins (woodwinds / brass): 0-based RR
        "--rrs": ("--rrs", range(int(os.environ.get("CS_RR_MAX", "6")))),
    }[kind]
    cands = list(cands)
    k = load(kon)
    tmp = tempfile.mkdtemp()
    traced = {c: render(binary, section, mid, flag, [c] * MAX_DRAWS, f"{tmp}/c{c}.wav", trace=True) for c in cands}
    renders = {c: v[0] for c, v in traced.items()}
    n = min(len(k), *(len(r) for r in renders.values()))
    ons = note_ons(mid)
    lo, hi = traced[cands[0]][1], traced[cands[-1]][1]
    # the onsets whose voices changed with the forced value drew
    drawn = sorted({round(a[0], 4) for a, b in zip(lo, hi) if a != b})
    windows = []
    for t0 in drawn:
        nxt = [t for t in ons if t > t0 + 1e-4]
        a = int(t0 * SR)
        windows.append((a, min(int(nxt[0] * SR) if nxt else n, n)))
    draws = []
    for a, b in windows:
        res = {c: energy(k[a:b] - renders[c][a:b]) for c in cands}
        draws.append(min(res, key=res.get))

    # A window can still hold the tail of an earlier drawn note (the second
    # key of an alternating burst), rendered above with the same forced
    # value: re-search such a draw (another drawn note within the second
    # before it) that leaves more than REFINE_DB, every other draw held at
    # its fit.
    ours, _ = render(binary, section, mid, flag, draws, out_wav)
    for i, (a, b) in enumerate(windows):
        # only a window that can hold an earlier drawn note's tail
        if not any(0 < a - a0 <= SR for a0, _ in windows[:i]):
            continue
        rel = energy(k[a:b] - ours[a:b]) / (energy(k[a:b]) + 1e-20)
        if 10 * np.log10(rel + 1e-20) <= REFINE_DB:
            continue
        best, best_e = draws[i], energy(k[a:b] - ours[a:b])
        for c in cands:
            if c == draws[i]:
                continue
            trial = draws[:i] + [c] + draws[i + 1 :]
            y, _ = render(binary, section, mid, flag, trial, f"{tmp}/r.wav")
            e = energy(k[a:b] - y[a:b])
            if e < best_e:
                best, best_e = c, e
        if best != draws[i]:
            draws[i] = best
            ours, _ = render(binary, section, mid, flag, draws, out_wav)
    print(",".join(str(v) for v in draws))
    print(f"{len(draws)} draws", file=sys.stderr)


if __name__ == "__main__":
    main()
