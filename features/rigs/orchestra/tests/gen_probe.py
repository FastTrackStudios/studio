#!/usr/bin/env python3
"""Write single-voice PROBE MIDI + manifest for the Kontakt null test.

    python3 gen_probe.py <out.mid> <out_manifest.tsv>

Each unit isolates as few Kontakt voices as the CSS group curves allow
(_ksp/CSS 1st Violins/groups.json): CC1=127 leaves only the ff dynamic
groups audible, CC2=0 only the nonvib groups (CC2=127 only vibsus), and a
velocity <= 64 takes the zone-1 first-note path (body only, sample offset 0,
no attack voice, no fade). CC59=0 first resets every round-robin counter.
Units are separated by silence like gen_css_ab.rs, so null_test.py and
identify.py window them the same way. 120 BPM, 600 PPQ: 1200 ticks/s, so
every time on the 5 ms grid is an exact tick (480 PPQ put 8.08 s 0.2 ms late,
enough to wreck a waveform fit).
"""
import struct, sys

PPQ = 600
TPS = 1200  # ticks per second at 120 BPM
GAP = 2.0


def vlq(v):
    out = [v & 0x7F]
    v >>= 7
    while v:
        out.append(0x80 | (v & 0x7F))
        v >>= 7
    return bytes(reversed(out))


class Smf:
    def __init__(self):
        self.ev = []  # (tick, order, bytes)

    def cc(self, t, num, val):
        self.ev.append((round(t * TPS), 0, bytes([0xB0, num, val])))

    def note(self, t, dur, key, vel):
        self.ev.append((round(t * TPS), 1, bytes([0x90, key, vel])))
        self.ev.append((round((t + dur) * TPS), -1, bytes([0x80, key, 0])))

    def write(self, path):
        body = vlq(0) + b"\xff\x51\x03" + (500000).to_bytes(3, "big")
        last = 0
        for t, _, b in sorted(self.ev, key=lambda e: (e[0], e[1])):
            body += vlq(t - last) + b
            last = t
        body += vlq(0) + b"\xff\x2f\x00"
        hdr = b"MThd" + struct.pack(">IHHH", 6, 0, 1, PPQ)
        open(path, "wb").write(hdr + b"MTrk" + struct.pack(">I", len(body)) + body)


def main():
    if len([a for a in sys.argv[1:] if not a.startswith("--")]) != 2:
        raise SystemExit(__doc__)
    m = Smf()
    rows = []
    t = 0.5
    m.cc(0.0, 59, 0)  # reset round-robins
    m.cc(0.0, 11, 127)
    m.cc(0.0, 64, 0)

    def unit(cat, desc, key, vel, cc1, cc2, hold, cc58=2):
        nonlocal t
        # controllers 1.5 s ahead: CC1/CC11 lag 120 ms, CC2 lags 1000 ms
        m.cc(t, 58, cc58)
        m.cc(t, 1, cc1)
        m.cc(t, 2, cc2)
        start = t + 1.5
        m.note(start, hold, key, vel)
        win = hold + GAP
        rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", key, cat, desc))
        t = start + win

    if "--tunes" in sys.argv:
        # One single-voice note (nonvib ff, CC1=127, CC2=0, velocity 40) per
        # playable key: Kontakt's effective playback rate per zone, to find
        # how it turns group × zone tune × semitones into a rate.
        for key in range(55, 99):
            unit("TUNE", f"nonvib ff only key{key} v40", key, 40, 127, 0, 3.0)
        m.write(sys.argv[1])
        with open(sys.argv[2], "w") as f:
            f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
            for r in rows:
                f.write("\t".join(map(str, r)) + "\n")
        print(f"{len(rows)} units, {t:.1f} s")
        return

    if "--legato" in sys.argv:
        # Two-note connected phrases (second note-on before the first
        # note-off) — CC1=127/CC2=0 leaves only the ff nonvib groups (NV
        # sustains + NV legato transitions). Zone 2 (vel 65-100) draws a random
        # body variant, so only zones 1 and 3 are probed (deterministic).
        def phrase(desc, a, b, vel, ioi, cc58):
            nonlocal t
            m.cc(t, 58, cc58)
            m.cc(t, 1, 127)
            m.cc(t, 2, 0)
            start = t + 1.5
            m.note(start, ioi + 0.05, a, vel)  # overlaps the next note-on by 50 ms
            m.note(start + ioi, 2.0, b, vel)
            win = ioi + 2.0 + 2.5
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", b, "LEG", desc))
            t = start + win

        for mode, cc58 in (("LL", 2), ("EX", 8)):
            for vel in (40, 120):
                for ioi in (0.08, 0.2, 1.0):
                    phrase(f"{mode} v{vel} 67->69 ioi{int(ioi*1000)}", 67, 69, vel, ioi, cc58)
                for a, b in ((67, 72), (69, 66), (67, 79), (67, 81)):
                    phrase(f"{mode} v{vel} {a}->{b} ioi500", a, b, vel, 0.5, cc58)
        m.write(sys.argv[1])
        with open(sys.argv[2], "w") as f:
            f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
            for r in rows:
                f.write("\t".join(map(str, r)) + "\n")
        print(f"{len(rows)} units, {t:.1f} s")
        return

    if "--curves" in sys.argv:
        # CC→volume law of the dynamic groups: one note per CC value, short
        # holds (only the steady body is fitted), nonvib only (CC2=0).
        for cc1 in list(range(0, 128, 4)) + [127]:
            unit("CURVE-CC1", f"nonvib key67 v40 cc1={cc1}", 67, 40, cc1, 0, 1.2)
        for cc11 in (0, 16, 32, 48, 64, 80, 96, 112, 127):
            m.cc(t, 11, cc11)
            unit("CURVE-CC11", f"nonvib ff key67 v40 cc11={cc11}", 67, 40, 127, 0, 1.2)
        m.cc(t, 11, 127)
        for cc2 in (0, 16, 32, 48, 64, 80, 96, 112, 127):
            unit("CURVE-CC2", f"vib/nonvib ff key67 v40 cc2={cc2}", 67, 40, 127, cc2, 1.2)
        m.write(sys.argv[1])
        with open(sys.argv[2], "w") as f:
            f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
            for r in rows:
                f.write("\t".join(map(str, r)) + "\n")
        print(f"{len(rows)} units, {t:.1f} s")
        return

    for key in (67, 69, 72, 79):
        unit("SUS-1V", f"nonvib ff only key{key} v40", key, 40, 127, 0, 4.0)
    for key in (67, 72):
        unit("SUS-1V", f"vibsus ff only key{key} v40", key, 40, 127, 127, 4.0)
    for key in (68, 70):
        unit("SUS-SHIFT", f"nonvib ff only key{key} v40 (resampled)", key, 40, 127, 0, 4.0)
    unit("SUS-LAYER", "nonvib CC1=90 key67 v40 (2 layers)", 67, 40, 90, 0, 4.0)
    unit("SUS-ATK", "nonvib ff key67 v100 (attack voice)", 67, 100, 127, 0, 4.0)
    unit("SUS-ATK", "nonvib ff key67 v120 (zone 3 attack)", 67, 120, 127, 0, 4.0)

    m.write(sys.argv[1])
    with open(sys.argv[2], "w") as f:
        f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
        for r in rows:
            f.write("\t".join(map(str, r)) + "\n")
    print(f"{len(rows)} units, {t:.1f} s")


if __name__ == "__main__":
    main()
