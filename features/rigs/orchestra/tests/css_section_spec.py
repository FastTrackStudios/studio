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
    print(")")


if __name__ == "__main__":
    main()
