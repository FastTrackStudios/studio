#!/usr/bin/env python3
"""Probe MIDI for a Shreddage null test: one articulation, single notes.

usage: shreddage_probe.py <out.mid> <mode> [--notes 1.2:0.4]

modes (keyswitch = the shipped TACT key):
  sus   Sustain (key 12)      stac  Staccato (key 13)      harm  Harmonics (key 15)

The keyswitch sounds at 0.5 s, the first note at 1.0 s. Each note lasts
`on` seconds with `gap` seconds of silence after it (default 1.2:0.4). The
sequence walks the range low → high → low at velocities that land in every
layer (Sustain edges 29/62/95, Staccato 41/85, Harmonics 64) and repeats
some notes, so a render exercises the string placement, every layer, the
per-position round robins and the anti-repetition draws. 120 BPM, 960 PPQ.
A manifest `<out>.tsv` lists each note: index, start s, end s, key, velocity.
"""
import struct, sys

KS = {"sus": 12, "stac": 13, "harm": 15}
PPQ, US_PER_Q = 960, 500_000


def vlq(n):
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append(0x80 | (n & 0x7F))
        n >>= 7
    return bytes(reversed(out))


def notes():
    """(key, velocity): the walk the probe plays."""
    keys = list(range(21, 73, 3)) + list(range(72, 20, -5))
    vels = [20, 50, 80, 110, 100, 40, 70, 127, 10, 60]
    seq = [(k, vels[i % len(vels)]) for i, k in enumerate(keys)]
    # repeats: the same key again and again (RR order, borrow draws)
    seq += [(40, 90)] * 6 + [(57, 64)] * 4 + [(33, 30)] * 4
    return seq


def main():
    out, mode = sys.argv[1], sys.argv[2]
    on, gap = 1.2, 0.4
    if "--notes" in sys.argv:
        on, gap = (float(x) for x in sys.argv[sys.argv.index("--notes") + 1].split(":"))
    tick = lambda s: round(s * PPQ * 1_000_000 / US_PER_Q)
    ev = [(tick(0.5), 0x90, KS[mode], 100), (tick(0.6), 0x80, KS[mode], 0)]
    rows, t = [], 1.0
    for i, (k, v) in enumerate(notes()):
        ev += [(tick(t), 0x90, k, v), (tick(t + on), 0x80, k, 0)]
        rows.append(f"{i}\t{t:.3f}\t{t + on:.3f}\t{k}\t{v}")
        t += on + gap
    ev.sort(key=lambda e: (e[0], e[1] != 0x80))
    body, last = bytearray(b"\x00\xff\x51\x03" + US_PER_Q.to_bytes(3, "big")), 0
    for tk, st, d1, d2 in ev:
        body += vlq(tk - last) + bytes([st, d1, d2])
        last = tk
    body += b"\x00\xff\x2f\x00"
    with open(out, "wb") as f:
        f.write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, PPQ))
        f.write(b"MTrk" + struct.pack(">I", len(body)) + body)
    with open(out[:-4] + ".tsv", "w") as f:
        f.write("i\tstart\tend\tkey\tvel\n" + "\n".join(rows) + "\n")
    print(f"{out}: {len(rows)} notes, {t + 1:.1f} s")


if __name__ == "__main__":
    main()
