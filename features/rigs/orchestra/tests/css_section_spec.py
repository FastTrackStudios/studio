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


def flex(groups, prefix):
    """The first unbypassed volume ENV_FLEX of the prefix's first group."""
    for name, g in groups.items():
        if name.startswith(prefix + " ") and " " not in name[len(prefix) + 1:]:
            for e in g["envelopes"]:
                if e["kind"] == "ENV_FLEX" and e["target"] == "volume" and not e["bypassed"]:
                    return e["segments"]
    return None


def short_layers(zones, artic):
    """Dynamics in band order and the RR count, from the unity Mix zones."""
    first, rrs = {}, {}
    for z in zones:
        n = os.path.basename(z["sample"])
        parts = n[:-4].split("_")
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
    groups = {g["name"][4:]: g for g in json.load(open(os.path.join(d, "groups.json"))) if g["name"].startswith("Mix ")}
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
        dyns, rr = short_layers(zones, fa)
        if not dyns and fa == "Pizzicato":
            # every section but 1st Violins names its pizzicato files "Pizz"
            dyns, rr = short_layers(zones, "Pizz")
        if not th or not db_ or not dyns:
            continue
        nb = (4 if ktuur[k] == 1 else 3) if kind == "ktuur" else k
        thresholds = th[1:nb]
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
    print(")")
    legato(pers)


def one(pers, var):
    v = pers.get(var)
    if not v:
        raise SystemExit(f"{var} missing from the persistent values")
    return v[0]


def marcato(pers):
    """The marcato knobs ($ocjln=6) and the first-note overlay table."""
    p = lambda v: one(pers, v)  # noqa: E731
    db = lambda v: fmt(p(v) / 10)  # noqa: E731  deci-dB
    print("    {\n        id Marcato")
    fizyd = pers.get("%fizyd", [])[:128]
    if len(fizyd) == 128:
        print(f"        overlay_cc1_scale ({' '.join(map(str, fizyd))})")
    print("        marcato_transition {")
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


def legato(pers):
    p = lambda v: one(pers, v)  # noqa: E731
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
