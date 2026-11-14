#!/usr/bin/env python3
"""Write a CSS section's decoded numbers as a `section.styx` overlay.

    python3 css_section_spec.py <_ksp/CSS Section dir> > "_patches/Section/section.styx"

Each Cinematic Studio Strings section ships its own velocity tables, RR
counts, group CC curves and envelopes in its NKI (persistent KSP values,
groups.json, zones.tsv — `nki --zones --out-dir`). The shared behaviour lives
in `specs/cinematic-strings.styx`; this emits only the per-section data,
which `PlayerPatch::load_merged` lays over it by articulation id
(`SectionOverlay`). Run on 1st Violins it must reproduce the hand-checked
values (the null tests do not move).
"""
import csv, json, os, re, sys

# articulation id -> (Mix group prefix, CCs of its volume tables)
BODY_GROUPS = {
    "Vibsus": ("vibsus", (1, 2)),
    "Nonvib": ("nonvib", (1, 2)),
    "Leg": ("legato", (1, 2)),
    "NVLeg": ("NVlegato", (1, 2)),
    "Marcato": ("marcato mod", (1,)),
    "Tremolo": ("tremolo", (1,)),
    "HTrills": ("trills", (1,)),
    "WTrills": ("trills", (1,)),
    "Harm": ("harmonics", (1,)),
}
RELEASE_GROUPS = {
    "Vsusrel": "vibsus rel",
    "NVrel": "nonvib rel",
    "Marcrel": "marcato mod rel",
    "HTrel": "half trills rel",
    "WTrel": "whole trills rel",
    "Tremrel": "tremolo rel",
    "Harmrel": "harmonics rel",
}
# short articulation -> (zone file articulation, threshold var, dB var, ktuur index or band count)
SHORTS = {
    "Spiccato": ("Spiccato", "%g1qri", "%bcez1", ("ktuur", 0)),
    "Staccatissimo": ("Staccatissimo", "%aoywv", "%ak5pm", ("ktuur", 1)),
    "Staccato": ("Staccato", "%nfxdy", "%opj4y", ("ktuur", 2)),
    "Sfz": ("Sfz", "%cjzwd", "%5u3ic", ("ktuur", 3)),
    "Pizzicato": ("Pizzicato", "%mjhzg", "%hu3zk", ("bands", 4)),
    "Bartokpizz": ("Bartokpizz", "%kgcee", "%eehua", ("bands", 2)),
    "Clegno": ("Clegno", "%fuiq3", "%a3xuw", ("bands", 3)),
}
# The editor knobs whose shipped values differ between sections (the
# 1st Violins values reproduce the shared spec). Every other KSP lerp table
# is identical across the five sections, or moves a breakpoint between two
# equal values (CC66 $acegr, CC67 $df322, CC68 $hn0qt, CC69 $ekasy/$kezb4).
# legtrans_OD: thresholds, soft anchors (zone 1), loud anchors (zone 2).
OVERLAP_DELAY = {
    "low_latency": (("$deey3", "$fxiox", "$jystg", "$zvaet"),
                    ("$nbkqa", "$mih5r", "$yzpsq", "$myv02"),
                    ("$55anl", "$umt5l", "$cffjr", "$yeo2q")),
    "expressive": (("$g45yq", "$bwkdm", "$waq1e", "$whtm2"),
                   ("$kadcz", "$nug53", "$tfwqt", "$xvurx"),
                   ("$lkn4u", "$ff0lo", "$vl1ol", "$qqqvj")),
}
# KSP RR sequences by RR count (declared in script_1).
SEQ_VARS = {4: "%okqzw", 5: "%5y220", 6: "%zs3om", 7: "%fl4bf"}


def persistent(d):
    out = {}
    for f in ("persistent_0.tsv", "persistent_1.tsv"):
        p = os.path.join(d, f)
        if os.path.exists(p):
            for line in open(p, encoding="utf-8", errors="replace"):
                parts = line.rstrip("\n").split("\t")
                if len(parts) >= 2:
                    out.setdefault(parts[0], [int(v) for v in parts[1].split() if re.fullmatch(r"-?\d+", v)])
    return out


def sequences(d):
    text = open(os.path.join(d, "script_1.ksp"), encoding="utf-8", errors="replace").read()
    out = {}
    for n, var in SEQ_VARS.items():
        m = re.search(r"declare " + re.escape(var) + r"\[\d+\] := \(([^)]*)\)", text)
        if m:
            out[n] = [int(v) for v in m.group(1).split(",")]
    return out


def fmt(v):
    return f"{v:.8g}"


def curves(groups, prefix, ccs, rel):
    """`group_curves` entries for every `Mix <prefix> <dyn>` group."""
    out = []
    for name, g in groups.items():
        if not name.startswith(prefix + " "):
            continue
        dyn = name[len(prefix) + 1:]
        if " " in dyn:
            continue
        for m in g["mods"]:
            if rel and m["source"] == "CC_TIME1" and m["cc"] == 36 and m["target"] == "fm7_level1" and m["curve"]:
                out.append((dyn, 36, m["curve"]))
            elif rel and m["source"] == "CC_VOLUME" and m["cc"] == 2 and m["curve"]:
                # Release groups crossfade by CC2 as well (vibrato side).
                out.append((dyn, 2, m["curve"]))
            elif not rel and m["source"] == "CC_VOLUME" and m["cc"] in ccs and m["curve"]:
                out.append((dyn, m["cc"], m["curve"]))
    return out


# Kontakt filter/EQ type ids the engine runs (`group_fx` stage kinds)
FX_KINDS = {2: "legacy_lp1", 6: "legacy_lp2", 50: "sv_lp1", 52: "sv_lp2", 55: "sv_lp4",
            22: "eq", 23: "eq", 24: "eq"}
FX_PARAMS = {"filterCutoff": "cutoff", "filterQ": "resonance",
             "eqGain1": "eq_gain1", "eqGain2": "eq_gain2", "eqGain3": "eq_gain3"}


def group_fx(d, all_groups, prefix):
    """`group_fx` entries `(dyn, stages, routes)` for every `Mix <prefix> <dyn>`
    group: its filter/EQ slots that are not bypassed, in slot order, and the
    controller routes onto them (decoded `group_fx.json` + `groups.json`)."""
    path = os.path.join(d, "group_fx.json")
    if not os.path.exists(path):
        return []
    slots = {x["group"]: x["slots"] for x in json.load(open(path))}
    out = []
    head = f"Mix {prefix} "
    for i, g in enumerate(all_groups):
        if not g["name"].startswith(head) or " " in g["name"][len(head):]:
            continue
        dyn = g["name"][len(head):]
        # an envelope moving a stage (marcato's attack sweep) is not run: such
        # a stage sits parked where the envelope starts, so leave it out
        swept = {e.get("slot") for e in g.get("envelopes", [])
                 if not e.get("bypassed") and e["target"] in FX_PARAMS and e.get("slot") is not None}
        stages, at = [], {}
        for s in sorted(slots.get(i, []), key=lambda s: s["slot"]):
            if s["kind"] != 7 or s.get("bypass", True):
                continue
            if s["slot"] in swept:
                print(f"{g['name']}: slot {s['slot']} moved by an envelope, not run", file=sys.stderr)
                continue
            kind = FX_KINDS.get(s.get("filter_type"))
            if kind is None:
                print(f"{g['name']}: live filter type {s.get('filter_type')} not run", file=sys.stderr)
                continue
            at[s["slot"]] = len(stages)
            stages.append((kind, s))
        routes = []
        for m in g["mods"]:
            if m.get("slot") in at and m["target"] in FX_PARAMS and m.get("source_kind") == "cc" \
                    and m["cc"] is not None:
                pts = m["curve"] if m["curve_kind"] == "points" and m.get("curve_enabled") else []
                if m["curve_kind"] == "steps" and m.get("curve_enabled"):
                    print(f"{g['name']}: step table on {m['target']} not run", file=sys.stderr)
                    continue
                # The decoded Invert flag is not applied: read as 1 − value it
                # shuts CSB 4 Horns' sustain filter at ff (probe 0 dB vs −16.8
                # without it); what it does is unverified (KONTRA ignores it too).
                sign = -1 if m.get("negative") else 1
                routes.append((at[m["slot"]], FX_PARAMS[m["target"]], m["cc"], sign * m["intensity"],
                               m["lag_ms"], pts))
        if stages:
            out.append((dyn, stages, routes))
    return out


def emit_group_fx(fx):
    """Print a `group_fx (...)` block (inside an articulation)."""
    if not fx:
        return
    print("        group_fx (")
    for dyn, stages, routes in fx:
        st = []
        for kind, s in stages:
            if kind == "eq":
                bands = " ".join(fmt(v) for b in s["eq_bands"] for v in b)
                st.append(f"{{ kind eq, bands ({bands}) }}")
            else:
                st.append(f"{{ kind {kind}, cutoff {fmt(s['cutoff'])}, resonance {fmt(s['resonance'])} }}")
        rt = []
        for stage, param, cc, inten, lag, pts in routes:
            r = f"{{ stage {stage}, param {param}, cc {cc}, intensity {fmt(inten)}, lag_ms {lag}"
            if pts:
                r += f", points ({' '.join(fmt(v) for v in pts)})"
            rt.append(r + " }")
        print(f"            {{ dynamic {dyn}, stages ({' '.join(st)}),")
        print(f"              routes ({' '.join(rt)}) }}")
    print("        )")


def flex(groups, prefix):
    """The first unbypassed volume ENV_FLEX of the prefix's first group."""
    for name, g in groups.items():
        if name.startswith(prefix + " ") and " " not in name[len(prefix) + 1:]:
            for e in g["envelopes"]:
                if e["kind"] == "ENV_FLEX" and e["target"] == "volume" and not e["bypassed"]:
                    return e["segments"]
    return None


GROUP_FAMILY = {"Spiccato": "spiccato", "Staccatissimo": "staccatissimo", "Staccato": "staccato",
                "Sfz": "sfz", "Pizzicato": "pizzicato", "Pizz": "pizzicato", "Bartokpizz": "bartok",
                "Clegno": "col legno"}


MICS = ("Mix", "Main", "Room", "Spot1", "Spot2")


def name_parts(sample):
    """A CSS sample stem split at `_`, mic back in third place where an NKI
    swapped it with the dynamic (Cellos `Ce_Spiccato_ff_Mix_C2_1`)."""
    parts = os.path.basename(sample)[:-4].split("_")
    if len(parts) > 3 and parts[2] not in MICS and parts[3] in MICS:
        parts[2], parts[3] = parts[3], parts[2]
    return parts


def group_layers(zones, groups, artic):
    """Dynamics in velocity order and the RR count of the short's own Mix
    group (a band whose own recordings exist on only a few keys still
    names the band: Violas Spiccato p/mf lend Staccatissimo elsewhere).
    None for a decode without the group column."""
    if not zones or "group" not in zones[0] or groups is None:
        return None
    gname = "Mix " + GROUP_FAMILY.get(artic, "")
    first, rrs = {}, {}
    for z in zones:
        if groups.get(int(z["group"])) != gname:
            continue
        parts = name_parts(z["sample"])
        if len(parts) < 5 or parts[1] != artic or parts[2] != "Mix":
            continue
        dyn = parts[3]
        first[dyn] = min(first.get(dyn, 999), int(z["vel_lo"]))
        rrs.setdefault(dyn, set()).add(parts[-1])
    if not first:
        return None
    return sorted(first, key=lambda k: first[k]), max(len(v) for v in rrs.values())


def short_layers(zones, artic):
    """Dynamics in band order and the RR count, from the unity Mix zones."""
    first, rrs = {}, {}
    for z in zones:
        parts = name_parts(z["sample"])
        if len(parts) < 5 or parts[1] != artic or parts[2] != "Mix" or abs(float(z["zvol"]) - 1) > 1e-3:
            continue
        dyn, rr = parts[3], parts[-1]
        v = int(z["vel_lo"])
        first[dyn] = min(first.get(dyn, 999), v)
        rrs.setdefault(dyn, set()).add(rr)
    dyns = sorted(first, key=lambda k: first[k])
    rr = max((len(v) for v in rrs.values()), default=0)
    return dyns, rr


def main():
    d = sys.argv[1]
    pers = persistent(d)
    seqs = sequences(d)
    all_groups = json.load(open(os.path.join(d, "groups.json")))
    groups = {g["name"][4:]: g for g in all_groups if g["name"].startswith("Mix ")}
    group_names = {i: g["name"] for i, g in enumerate(all_groups)}
    zones = list(csv.DictReader(open(os.path.join(d, "zones.tsv")), delimiter="\t"))
    print(f"// Generated by css_section_spec.py from {os.path.basename(os.path.normpath(d))} — do not edit.")
    print("articulations (")
    for aid, (prefix, ccs) in BODY_GROUPS.items():
        cs = curves(groups, prefix, ccs, rel=False)
        if not cs:
            continue
        print(f"    {{\n        id {aid}")
        print("        group_curves (")
        for dyn, cc, pts in cs:
            print(f"            {{ dynamic {dyn}, cc {cc}, points ({' '.join(fmt(v) for v in pts)}) }}")
        print("        )")
        print("    }")
    for aid, prefix in RELEASE_GROUPS.items():
        cs = curves(groups, prefix, (), rel=True)
        if not cs:
            continue
        print(f"    {{\n        id {aid}")
        print("        group_curves (")
        for dyn, cc, pts in cs:
            print(f"            {{ dynamic {dyn}, cc {cc}, points ({' '.join(fmt(v) for v in pts)}) }}")
        print("        )")
        print("    }")
    ktuur = pers.get("%ktuur", [1, 1, 1, 0])
    for aid, (fa, tvar, dvar, (kind, k)) in SHORTS.items():
        th, db_ = pers.get(tvar), pers.get(dvar)
        gl = group_layers(zones, group_names, fa) or (group_layers(zones, group_names, "Pizz") if fa == "Pizzicato" else None)
        dyns, rr = gl if gl else short_layers(zones, fa)
        if not dyns and fa == "Pizzicato":
            # every section but 1st Violins names its pizzicato files "Pizz"
            dyns, rr = short_layers(zones, "Pizz")
        if not th or not db_ or not dyns:
            continue
        nb = (4 if ktuur[k] == 1 else 3) if kind == "ktuur" else k
        thresholds = th[1:nb]
        if gl and len(dyns) > nb:
            # bands play velocity blocks from the bottom: a block past the
            # last band is never reached (Cellos Spiccato's ff, %ktuur 0)
            dyns = dyns[:nb]
        print(f"    {{\n        id {aid}")
        print(f"        dynamics ({' '.join(dyns)})")
        print(f"        rr {rr}")
        print(f"        vel_thresholds ({' '.join(map(str, thresholds))})")
        print(f"        vel_layer_db ({' '.join(map(str, db_[:nb]))})")
        if aid in ("Sfz", "Bartokpizz"):
            print("        rr_per_note true")
        elif rr in seqs:
            print(f"        rr_sequence ({' '.join(map(str, seqs[rr]))})")
        print("    }")
    marcato(pers)
    pizz_release(pers, groups)
    print(")")
    legato(pers, os.path.basename(os.path.normpath(d)))


def one(pers, var):
    v = pers.get(var)
    if not v:
        raise SystemExit(f"{var} missing from the persistent values")
    return v[0]


# The pizz family's release knob ($3kbfg -> CC31 -> the group flex's third
# segment time). Measured in Kontakt (Cellos/Basses pizz bursts): 75 gives
# 1470 ms; the violins' and Violas' 127 the full 7381. Other values follow
# the fitted power law.
PIZZ_RELEASE_MS = {127: 7381.0, 75: 1470.0}


def pizz_release(pers, groups):
    """`amp_env` for the pizz family when the section shortens its release."""
    knob = one(pers, "$3kbfg")
    g = groups.get("pizzicato") or {}
    segs = next((e["segments"] for e in g.get("envelopes", [])
                 if e["kind"] == "ENV_FLEX" and e["target"] == "volume" and not e["bypassed"]), None)
    if knob == 127 or not segs or len(segs) < 3:
        return
    t3 = PIZZ_RELEASE_MS.get(knob, segs[2][0] * (knob / 127) ** 3.06)
    segs = [list(segs[0]), list(segs[1]), [t3, segs[2][1], segs[2][2]]]
    env = " ".join(f"{{time_ms {fmt(float(t))}, level {fmt(float(l))}, curve {fmt(float(c))}}}" for t, l, c in segs)
    for aid in ("Pizzicato", "Bartokpizz"):
        print(f"    {{\n        id {aid}")
        print(f"        amp_env ( {env} )")
        print("    }")


def marcato(pers):
    """The marcato knobs ($ocjln=6) and the first-note overlay table."""
    p = lambda v: one(pers, v)  # noqa: E731
    db = lambda v: fmt(p(v) / 10)  # noqa: E731  deci-dB
    print("    {\n        id Marcato")
    fizyd = pers.get("%fizyd", [])[:128]
    if len(fizyd) == 128:
        print(f"        overlay_cc1_scale ({' '.join(map(str, fizyd))})")
    print("        marcato_transition {")
    # $kbqnb: hard above velocity $qm4n3 and IOI $hx3nl
    print(f"            hard_velocity {p('$qm4n3')}")
    print(f"            hard_ioi_ms {p('$hx3nl')}")
    # $1fvjk for an interval 1-12: soft $yp2sq, hard $po1el
    print(f"            body_offset_ms ({p('$yp2sq')} {p('$po1el')})")
    # LT start: $ggt00/$v0rbb/$5exar over $yam53/$nzsuf/$5c2um
    print(f"            lt_offset_ms (({p('$yam53')} {p('$ggt00')}) ({p('$nzsuf')} {p('$v0rbb')}) ({p('$5c2um')} {p('$5exar')}))")
    # LT Vol: interval <= 2 $ycnsk -> $nqjls, wider $0uoxp -> $i0s2b, over $14fyb..$3zzii
    print(f"            lt_db_step (({p('$14fyb')} {db('$ycnsk')}) ({p('$3zzii')} {db('$nqjls')}))")
    print(f"            lt_db_leap (({p('$14fyb')} {db('$0uoxp')}) ({p('$3zzii')} {db('$i0s2b')}))")
    # CC18: hard within an octave, > octave soft, > octave hard
    print(f"            cc18_other ({p('$2ld3s')} {p('$bkgr5')} {p('$cdv4h')})")
    print("        }")
    print("    }")


def legato(pers, section):
    p = lambda v: one(pers, v)  # noqa: E731
    # %dtnvu: the violin patches ($sxwuz 1-2) pair each even key with the
    # key below for per-note round robins; the other sections count per key.
    print(f"rr_per_key {'false' if 'Violins' in section else 'true'}")
    print("legato {")
    print("    overlap_delay {")
    for mode, (th, soft, loud) in OVERLAP_DELAY.items():
        t = " ".join(str(p(v)) for v in th)
        print(f"        {mode} {{")
        print(f"            soft {{ thresholds_ms ({t}), anchors_ms ({' '.join(str(p(v)) for v in soft)}) }}")
        print(f"            loud {{ thresholds_ms ({t}), anchors_ms ({' '.join(str(p(v)) for v in loud)}) }}")
        print("        }")
    print("    }")
    # %v2aqt[$2ekvq]: the loud first note's body-variant counter as shipped
    print(f"    first_note_variant_init {p('$2ekvq')}")
    # zone 2 / zone 3 body fade-in after the mute
    print(f"    first_note_fade_ms ({p('$otlef')} {p('$yqhyd')})")
    print("}")


if __name__ == "__main__":
    main()


def instrument_gain_db(d):
    """Kontakt's level over the raw file, decoded: the program volume
    (program.json) x the Mix bus fader x the gain stages (effect type 4,
    the value 10 bytes into the slot) on the Mix bus, the last bus that
    carries one (buses.json). None when the decode has no buses."""
    import json, math, struct
    try:
        prog = json.load(open(os.path.join(d, "program.json")))
        buses = json.load(open(os.path.join(d, "buses.json")))
    except (OSError, ValueError):
        return None
    mix = None
    for b in buses:
        if b.get("index") is None:
            continue
        gains = []
        for chain in b.get("chains", []):
            for slot in chain:
                if slot.get("kind") == 4:
                    h = bytes.fromhex(slot.get("priv_hex", ""))
                    if len(h) >= 14:
                        gains.append(struct.unpack("<f", h[10:14])[0])
        if gains:
            mix = (b["pub"][0], gains)
    if mix is None:
        return None
    lin = prog["volume"] * mix[0]
    for g in mix[1]:
        lin *= g
    return round(20 * math.log10(lin), 2)
