#!/usr/bin/env python3
"""Write a CS Woodwinds instrument's decoded numbers as a `section.styx` overlay.

    python3 csw_instrument_spec.py <_ksp/CSW <instrument> dir> > "_patches/<label>/section.styx"

The woodwinds counterpart of css_section_spec.py. CSW runs the strings'
engine and group layout with its own recordings in the strings' slots
(Flutter in "harmonics", Repetitions in "spiccato", NVsus in "nonvib"), so
the readers are the strings generator's; what differs is which KSP variable
holds a knob (tests/cs_ksp_var_map.tsv):

- shorts: the same threshold / dB tables; random round robins that avoid the
  key's last two (no fixed sequences);
- marcato: no hard-attack path, one flat transition start offset
  (`$5ibvn`) and one flat transition volume (`$b23jt`);
- legato: the same overlap-delay tables; no first-note mute / fade / variant
  counter (removed library-wide by derive_cs_spec.py).
"""
import csv, json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import css_section_spec as css  # noqa: E402

BODY_GROUPS = {
    "Vibsus": ("vibsus", (1, 2)),
    "NVsus": ("nonvib", (1, 2)),
    "Leg": ("legato", (1, 2)),
    "NVLeg": ("NVlegato", (1, 2)),
    "Marcato": ("marcato mod", (1,)),
    "HTrills": ("trills", (1,)),
    "WTrills": ("trills", (1,)),
    "Flutter": ("harmonics", (1,)),
}
RELEASE_GROUPS = {
    "Vibsusrel": "vibsus rel",
    "NVsusrel": "nonvib rel",
    "Marcrel": "marcato mod rel",
    "HTrel": "half trills rel",
    "WTrel": "whole trills rel",
    "Fluttrel": "harmonics rel",
}
# short articulation -> (zone file articulation, group family, threshold var, dB var)
SHORTS = {
    "Repetitions": ("Repetitions", "spiccato", "%g1qri", "%bcez1"),
    "Staccatissimo": ("Staccatissimo", "staccatissimo", "%aoywv", "%ak5pm"),
    "Staccato": ("Staccato", "staccato", "%nfxdy", "%opj4y"),
    "Sfz": ("Sfz", "sfz", "%cjzwd", "%5u3ic"),
}
# KSP: random(1,n) until it is neither of the key's last two picks
RR_AVOID_LAST = 2
MICS = ("Mix", "Main", "Room", "Close", "OH")


def name_parts(sample):
    parts = os.path.basename(sample)[:-4].split("_")
    if len(parts) > 3 and parts[2] not in MICS and parts[3] in MICS:
        parts[2], parts[3] = parts[3], parts[2]
    return parts


def short_layers(zones, group_names, artic, family):
    """Dynamics in velocity order and the RR count of the short's own Mix group."""
    gname = "Mix " + family
    first, rrs = {}, {}
    for z in zones:
        if group_names.get(int(z["group"])) != gname:
            continue
        parts = name_parts(z["sample"])
        if len(parts) < 5 or parts[1] != artic or parts[2] != "Mix":
            continue
        dyn = parts[3]
        first[dyn] = min(first.get(dyn, 999), int(z["vel_lo"]))
        rrs.setdefault(dyn, set()).add(parts[-1])
    if not first:
        return [], 0
    return sorted(first, key=lambda k: first[k]), max(len(v) for v in rrs.values())


# The vibrato-less instruments put their NVsus recordings in the vibsus slot
# (slot_relabel.py makes those zones Vibsus): the slot's curves are theirs.
SLOT_ALSO = {
    "Vibsus": {"Vibsus", "NVsus"}, "Vibsusrel": {"Vibsusrel", "NVsusrel"},
    # CSB 4 Horns: the mute slots hold Stopped recordings (slot_relabel.py)
    "Mutesus": {"Mutesus", "Stoppedsus"}, "Mutesusrel": {"Mutesusrel", "Stoppedsusrel"},
}


def recorded_dynamics(zones, group_names, prefix, artic):
    """{group dynamic: recorded dynamic} for the Mix `<prefix> <dyn>` groups
    that play `artic`: CSW puts its recordings in the strings' slots under
    the strings' names (Flutter p / ff in "harmonics pp" / "harmonics mp")."""
    count = {}
    for z in zones:
        g = group_names.get(int(z["group"]), "")
        if not g.startswith(f"Mix {prefix} "):
            continue
        gdyn = g[len(f"Mix {prefix} "):]
        if " " in gdyn:
            continue
        parts = name_parts(z["sample"])
        if len(parts) < 4 or parts[1] not in SLOT_ALSO.get(artic, {artic}) or parts[2] != "Mix":
            continue
        c = count.setdefault(gdyn, {})
        c[parts[3]] = c.get(parts[3], 0) + 1
    return {g: max(c, key=c.get) for g, c in count.items()}


def emit_curves(aid, cs, dyns=None, fx=()):
    print(f"    {{\n        id {aid}")
    if dyns:
        print(f"        dynamics ({' '.join(dyns)})")
    print("        group_curves (")
    for dyn, cc, pts in cs:
        print(f"            {{ dynamic {dyn}, cc {cc}, points ({' '.join(css.fmt(v) for v in pts)}) }}")
    print("        )")
    css.emit_group_fx(fx)
    print("    }")


def layer_fx(d, all_groups, prefix, rec):
    """`group_fx` of `prefix`'s Mix groups, each named by its recorded dynamic."""
    return [(rec[dy], st, rt) for dy, st, rt in css.group_fx(d, all_groups, prefix) if dy in rec]


# Kontakt's level over the raw file per instrument, null-tested (Kontakt minus
# ours on the probe's held notes, + the library's 2.55): the NKI's instrument
# volume, which the decoder does not read yet.
INSTRUMENT_GAIN_DB = {
    "CSW Solo Clarinet": -5.99, "CSW Solo Contra Bassoon": -6.00, "CSW Solo Bass Clarinet": -6.02,
    "CSW Solo Flute": -6.02, "CSW Solo Bassoon": -6.05, "CSW Solo Cor Anglais": -6.06,
    "CSW Solo Alto Flute": -6.06, "CSW Solo Oboe": -5.03, "CSW Solo Piccolo": -4.05,
    "CSW 2 Clarinets": 1.03, "CSW 2 Flutes": -3.22, "CSW 2 Oboes": -1.98, "CSW 2 Bassoons": -5.09,
}


def main():
    d = sys.argv[1]
    pers = css.persistent(d)
    all_groups = json.load(open(os.path.join(d, "groups.json")))
    groups = {g["name"][4:]: g for g in all_groups if g["name"].startswith("Mix ")}
    group_names = {i: g["name"] for i, g in enumerate(all_groups)}
    zones = list(csv.DictReader(open(os.path.join(d, "zones.tsv")), delimiter="\t"))
    print(f"// Generated by csw_instrument_spec.py from {os.path.basename(os.path.normpath(d))} — do not edit.")
    # decoded (program volume x Mix bus fader x its gain stage), else the
    # measured table
    gain = css.instrument_gain_db(d)
    if gain is None:
        gain = INSTRUMENT_GAIN_DB.get(os.path.basename(os.path.normpath(d)))
    if gain is not None:
        print(f"instrument_gain_db {gain}")
    # the vibrato CC ($vkgua; -1 = none: CC2 never leaves the vibsus slot)
    vk = css.one(pers, "$vkgua")
    print(f'vibrato_controller "{"" if vk is None or int(vk) < 0 else f"CC{int(vk)}"}"')
    print("articulations (")
    for aid, (prefix, ccs) in BODY_GROUPS.items():
        # name each layer by the dynamic its recordings carry
        rec = recorded_dynamics(zones, group_names, prefix, aid)
        cs = [(rec[dyn], cc, pts) for dyn, cc, pts in css.curves(groups, prefix, ccs, rel=False) if dyn in rec]
        if cs:
            dyns = list(dict.fromkeys(dyn for dyn, _, _ in cs))
            emit_curves(aid, cs, dyns, layer_fx(d, all_groups, prefix, rec))
    for aid, (fa, family, tvar, dvar) in SHORTS.items():
        th, db_ = pers.get(tvar), pers.get(dvar)
        dyns, rr = short_layers(zones, group_names, fa, family)
        if not th or not db_ or not dyns:
            continue
        nb = min(len(dyns), 4)
        print(f"    {{\n        id {aid}")
        print(f"        dynamics ({' '.join(dyns[:nb])})")
        print(f"        rr {rr}")
        print(f"        vel_thresholds ({' '.join(map(str, th[1:nb]))})")
        print(f"        vel_layer_db ({' '.join(map(str, db_[:nb]))})")
        # with RR_AVOID_LAST or fewer RRs the KSP draw could never finish;
        # such a short (Sfz, 2 RRs) must take another path — UNVERIFIED
        print(f"        rr_random_avoid_last {min(RR_AVOID_LAST, max(rr - 1, 0))}")
        # a short the key has not struck within $5i0r1 ms detunes by a
        # random ±$14yum cents (KSP: `$thtao`, ENGINE_UPTIME - last strike)
        if css.one(pers, "$thtao") == 1 and css.one(pers, "$14yum"):
            print(f"        random_tune_cents {css.one(pers, '$14yum')}")
            print(f"        random_tune_gate_ms {css.one(pers, '$5i0r1')}")
        css.emit_key_pitch(css.key_pitch_cents(all_groups, family))
        print("    }")
    for aid, prefix in RELEASE_GROUPS.items():
        rec = recorded_dynamics(zones, group_names, prefix, aid)
        cs = [(rec[dyn], cc, pts) for dyn, cc, pts in css.curves(groups, prefix, (), rel=True) if dyn in rec]
        if cs:
            emit_curves(aid, cs, list(dict.fromkeys(dyn for dyn, _, _ in cs)),
                        layer_fx(d, all_groups, prefix, rec))
    marcato(pers)
    print(")")
    legato(pers, zones, group_names)


def marcato(pers):
    p = lambda v: css.one(pers, v)  # noqa: E731
    off = p("$5ibvn")  # the one LT start offset (CSS: $ggt00 / $v0rbb / $5exar by IOI)
    vol = css.fmt(p("$b23jt") / 10)  # the one LT volume, deci-dB (CSS: 4 by IOI and interval)
    print("    {\n        id Marcato")
    print("        marcato_transition {")
    print("            hard_velocity 127")  # no hard-attack path in CSW
    print(f"            body_offset_ms ({p('$yp2sq')} {p('$yp2sq')})")
    print(f"            lt_offset_ms ((0 {off}))")
    print(f"            lt_db_step ((0 {vol}))")
    print(f"            lt_db_leap ((0 {vol}))")
    print("        }")
    print("    }")


DYN_ORDER = ("pp", "p", "mp", "mf", "f", "ff", "fff")


def body_handoff(pers, zones, group_names):
    """Per-layer body handoff from `%k1aqh` (KSP 13030-13050, the CSB
    mechanism): body slots 0-2 are the vibsus groups and 3-5 the nonvib
    groups, each in dynamic order; per slot `[big wait, wait z1-3, big fade,
    fade z1-3]`. The layer is named by the recordings its group holds."""
    arr = list(pers.get("%k1aqh", []))
    arr += [0] * (200 - len(arr))
    out = []
    for first, prefix, aid in ((0, "vibsus", "Vibsus"), (3, "nonvib", "NVsus")):
        rec = recorded_dynamics(zones, group_names, prefix, aid)
        for k, gdyn in enumerate(sorted(rec, key=DYN_ORDER.index)):
            b = (first + k) * 20
            wait, fade = arr[b + 1 : b + 4], arr[b + 5 : b + 8]
            if not any(wait):
                continue
            out.append(
                f"        {{ articulation {aid}, dynamic {rec[gdyn]}, wait_ms ({' '.join(map(str, wait))}), "
                f"fade_ms ({' '.join(map(str, fade))}), big_fade_ms {arr[b + 4]} }}"
            )
    return out


def legato(pers, zones=None, group_names=None):
    p = lambda v: css.one(pers, v)  # noqa: E731
    print("legato {")
    # the transition's entry by velocity zone (KSP 9463-9478: low latency
    # $ak2j4/$ixzi1/$cltif, expressive $fjf3c/$2p1wl/$ywj0r; zone 1 adds the
    # overlap delay, `overlap_offset_zone1_only` in the library spec)
    print(f"    lt_offset_low_latency ({p('$ak2j4')} {p('$ixzi1')} {p('$cltif')})")
    print(f"    lt_offset_expressive ({p('$fjf3c')} {p('$2p1wl')} {p('$ywj0r')})")
    if zones is not None:
        rows = body_handoff(pers, zones, group_names)
        if rows:
            print("    body_handoff (")
            print("\n".join(rows))
            print("    )")
    print("    overlap_delay {")
    for mode, (th, soft, loud) in css.OVERLAP_DELAY.items():
        t = " ".join(str(p(v)) for v in th)
        print(f"        {mode} {{")
        print(f"            soft {{ thresholds_ms ({t}), anchors_ms ({' '.join(str(p(v)) for v in soft)}) }}")
        print(f"            loud {{ thresholds_ms ({t}), anchors_ms ({' '.join(str(p(v)) for v in loud)}) }}")
        print("        }")
    print("    }")
    print("}")


if __name__ == "__main__":
    main()
