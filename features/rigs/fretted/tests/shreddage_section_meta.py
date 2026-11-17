#!/usr/bin/env python3
"""Shreddage 3.5 with shipped TACT meta (Argent): the decode's numbers →
`_patches/<instrument>/section.styx`. Called by shreddage_section.py when
the NKI ships `%_<lib>_meta`; the model is the library's
`_notes/engine-model.md` (Argent: the delta over Abyss).

Every number comes from the decode: persistent_0.tsv (shipped values),
script_0.ksp, groups.tsv, program.json, buses.json. Envelope stage times
and levels follow Kontakt's parameter laws as KONTRA states them
(crates/base/law/src/common.rs): attack 2·(7501^x − 1) ms, decay and
release 2·(12501^x − 1) ms, sustain x³, x = engine units / 1e6.
"""
import json, math, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shreddage_zones import META_NAMES  # noqa: E402
from shreddage_section import (FALL_K, LAG_MS, attack_k, fmt, ints, ksp_list,  # noqa: E402
                               persistent)

# Kontakt filter/EQ type ids the engine runs (`group_fx` stage kinds), and
# the modulation targets it moves (as features/rigs/orchestra/tests/
# css_section_spec.py maps them).
FX_KINDS = {2: "legacy_lp1", 6: "legacy_lp2", 50: "sv_lp1", 52: "sv_lp2", 55: "sv_lp4",
            22: "eq", 23: "eq", 24: "eq"}
FX_PARAMS = {"filterCutoff": "cutoff", "filterQ": "resonance",
             "eqGain1": "eq_gain1", "eqGain2": "eq_gain2", "eqGain3": "eq_gain3"}


def fx_fmt(x):
    t = f"{x:.6f}".rstrip("0").rstrip(".")
    return t if t not in ("-0", "") else "0"


def group_fx(slots, g):
    """One group's `group_fx` entry (every layer): its unbypassed filter/EQ
    slots in slot order and the controller routes onto them."""
    stages, at = [], {}
    for x in sorted(slots.get(g["index"], []), key=lambda x: x["slot"]):
        if x["kind"] != 7 or x.get("bypass", True):
            continue
        kind = FX_KINDS.get(x.get("filter_type"))
        if kind is None:
            sys.exit(f"{g['name']}: filter type {x.get('filter_type')} not run")
        at[x["slot"]] = len(stages)
        if kind == "eq":
            bands = " ".join(fx_fmt(v) for b in x["eq_bands"] for v in b)
            stages.append(f"{{ kind eq, bands ({bands}) }}")
        else:
            stages.append(f"{{ kind {kind}, cutoff {fx_fmt(x['cutoff'])}, resonance {fx_fmt(x['resonance'])} }}")
    routes = []
    for m in g["mods"]:
        if m.get("slot") in at and m["target"] in FX_PARAMS and m.get("source_kind") == "cc":
            sign = -1 if m.get("negative") else 1
            r = (f"{{ stage {at[m['slot']]}, param {FX_PARAMS[m['target']]}, cc {m['cc']}, "
                 f"intensity {fx_fmt(sign * m['intensity'])}, lag_ms {m['lag_ms']}")
            if m["curve_kind"] == "points" and m.get("curve_enabled"):
                r += f", points ({' '.join(fx_fmt(v) for v in m['curve'])})"
            routes.append(r + " }")
    if not stages:
        return []
    return ['        group_fx (',
            f'            {{ dynamic "", stages ({" ".join(stages)}),',
            f'              routes ({" ".join(routes)}) }}',
            '        )']


# Script-side articulation lists by their KSP names (L641-653).
LISTS = ("s3__exclude_artic_all_releases", "s3__hammer_sources", "s3__hammerable_artics")


def stage_ms(units, base):
    return 2.0 * (base ** (units / 1e6) - 1.0)


def tension(k):
    """Our flex segment's tension for a Kontakt segment law k."""
    return 0.5 - k / 15.0


def env(attack_u, curve, decay_u=None, sustain_u=None):
    """[(ms, level, tension)]: the 33-frame lag, the attack, and a decay to
    the sustain level when it is below full (KONTRA's stage laws)."""
    a = stage_ms(attack_u, 7501.0)
    lag = min(LAG_MS, a)
    segs = [(lag, 0.0, 0.5), (a - lag, 1.0, tension(attack_k(curve)))]
    if sustain_u is not None and sustain_u < 1_000_000:
        segs.append((stage_ms(decay_u, 12501.0), (sustain_u / 1e6) ** 3, tension(FALL_K)))
    return segs


def env_line(segs):
    return " ".join(f"{{time_ms {fmt(t)}, level {fmt(l)}, curve {c:.4f}}}" for t, l, c in segs)


def vel_law(vel_db, curve, vv):
    """%vel_db by Vel->Vol `vv` along the velocity curve, −1 mdB (L8028), dB."""
    out = []
    for v in range(128):
        c = curve[max(v, 1)]
        lo, fr = vv // 100, vv % 100
        hi = lo + (0 if lo == 10 else 1)
        mdb = int(((100 - fr) * vel_db[c * 11 + lo] + fr * vel_db[c * 11 + hi]) / 100) - 1
        out.append(fmt(mdb / 1000))
    return out


def main(root, nki, inst, lib):
    k = os.path.join(root, "_ksp", nki)
    p = persistent(os.path.join(k, "persistent_0.tsv"))
    script = open(os.path.join(k, "script_0.ksp")).read()
    names = META_NAMES[lib]
    na = len(names)
    groups = {}
    with open(os.path.join(k, "groups.tsv")) as f:
        head = f.readline().rstrip("\n").split("\t")
        for line in f:
            r = dict(zip(head, line.rstrip("\n").split("\t")))
            groups[int(r["idx"])] = r
    npickups = len({r["name"].split(" ", 1)[0] for r in groups.values()})
    block = len(groups) // npickups
    gjson = json.load(open(os.path.join(k, "groups.json")))
    gjson = {x["index"]: x for x in (gjson if isinstance(gjson, list) else gjson["groups"])}
    slots = {x["group"]: x["slots"] for x in json.load(open(os.path.join(k, "group_fx.json")))}

    ep = ints(p, "%s3__engine_pars", 90)
    meta = ints(p, f"%_{lib}_meta", 64 * na)
    matrix = ints(p, "%tact___matrix", 128 * na)
    table = ints(p, "%tact___table_matrix", 512 * na)
    vel_db = ints(p, "%vel_db", 1408)
    curve = ints(p, "%s3__engine_par_vel_curve", 128)
    rr_seed = ints(p, "%s3___rr_seed", 4096 * 7)
    smeta = ints(p, "%str___string_meta")
    opens = smeta[0::2]
    M = lambda a, i: meta[64 * a + i]
    X = lambda a, i: matrix[128 * a + i]
    lists = {n: ksp_list(script, n) for n in LISTS}

    # The pickup the script lets play (engine_pars[3] = 0: the last block of
    # groups, Bridge; `g / block = npickups − 1 − [3]`, Argent L7270,
    # Darkwall L7131) and its Console bus.
    pickup = npickups - 1 - ep[3]
    program = json.load(open(os.path.join(k, "program.json")))
    buses = json.load(open(os.path.join(k, "buses.json")))
    bus = next(b for b in buses if b["index"] == pickup)
    gain_db = 20 * math.log10(program["volume"]) + 20 * math.log10(bus["pub"][0])
    group = lambda a: groups[pickup * block + M(a, 1)]
    gname = lambda word: next(r for r in groups.values()
                              if int(r["idx"]) // block == pickup and r["name"].split(" ", 1)[1] == word)

    capo = ep[77]
    if ep[4] <= 0:
        sys.exit("automatic hand position: not modelled yet")
    hand = max(ep[4], capo) - 1
    mode = ep[11]
    hier = [int(x) for x in re.search(r"declare %str___hierarchy\[24\] := \(([^)]*)\)", script)
            .group(1).split(",")][6 * mode: 6 * mode + 6]
    widths = [4] + [opens[s - 1] - opens[s] - 1 for s in range(1, len(opens))]
    seed = int(re.search(r"declare \$math__rand_seed := (\d+)", script).group(1))
    mult = int(re.search(r"\$math__rand_seed := (\d+)\*\$math__rand_seed\+1", script).group(1))

    # Multitracking (engine_pars[22..25], s3__engine__process_multitracking L7099)
    on = [t for t in range(4) if ep[22 + t] == 1]
    pans = {1: [0.0], 2: [-1.0, 1.0], 3: [1.0, 0.0, 1.0], 4: [-1.0, -0.85, 0.85, 1.0]}[len(on)]
    if on != list(range(len(on))):
        sys.exit(f"tracks {on}: only the first n tracks are modelled")
    if ep[20] == 1:
        sys.exit("anti-repetition on: its track rules (L7538) are not modelled yet")

    # picked: their own RR stream per stroke; a release keeps the unpicked
    # counter whatever its meta says (Darkwall PR: `[1680 + 24s + f]`)
    picked = [names[a] for a in range(na) if M(a, 35) == 1 and M(a, 21) not in (6, 7)]
    excluded = [names[a] for a in lists["s3__exclude_artic_all_releases"]]
    pr_sources = [names[a] for a in range(na) if M(a, 40) == 1 and names[a] not in excluded
                  and M(a, 21) not in (6, 7)]
    unp = names.index("UnpitchedRelease")
    # a release's x3 is multiplied by engine_pars[14] (0) in its own path
    offsets = [(names[a], X(a, 3) * M(a, 6) / 10 / 1000) for a in range(na)
               if X(a, 3) > 0 and M(a, 21) not in (6, 7)]

    # the knobs the script holds on controllers (`set_controller`, L9167-9170)
    ccs = list(dict((int(c), ep[int(i)]) for c, i in
                    re.findall(r"set_controller\((\d+),%s3__engine_pars\[(\d+)\]\)", script)).items())
    L = [f"// Generated by features/rigs/fretted/tests/shreddage_section_meta.py from {nki} — do not edit.",
         f"// program volume {program['volume']} × pickup bus {bus['index']} {bus['pub'][0]}",
         f"instrument_gain_db {fmt(gain_db)}",
         "// set_controller(cc, engine_pars[i]): pickup volume, tone, bite, vibrato",
         f"fixed_ccs ({' '.join(f'{{cc {c}, value {v}}}' for c, v in ccs)})",
         "fretboard {",
         f"    strings ({' '.join(map(str, opens))})",
         "    frets 24",
         f"    hand_fret {hand}",
         f"    hand_widths ({' '.join(map(str, widths))})",
         f"    // fretting mode {mode}: 1 << %str___hierarchy[{6 * mode + 1}..{6 * mode + 5}]",
         f"    weights {{ in_hand {1 << hier[1]}, free {1 << hier[2]}, adjacent {1 << hier[3]}, "
         f"same_string {1 << hier[4]}, same_fret {1 << hier[5]} }}",
         # engine_pars[60]: reset_hand on transport start sets %last_frets to −1
         f"    fresh_last_fret {-1 if ep[60] == 1 else 0}",
         "    rr_slot_stride 24",
         f"    onset_delay_us {51 + 1000 * ep[26]}",
         "    retrigger_fade_ms 400",
         # anti-repetition off (engine_pars[20] = 0): the LCG still steps once
         # per play_event (L7199), never borrowing
         f"    borrow {{ chance_permille 0, seed {seed}, multiplier {mult}, frets 2 }}",
         # release_event: %tact___meta[64·a + 40] and not exclude_all_releases,
         # held engine_pars[50]..[51] ms (L5236-5250)
         f"    release_layer {{ articulation PitchedRelease, sources ({' '.join(pr_sources)}), "
         f"min_held_ms {ep[50]}, max_held_ms {ep[51]} }}",
         # unpitched release (L5286-5310): key (counter + track) mod RR count,
         # half the source's script volume, held engine_pars[52]..[53] ms
         f"    noise_release {{ articulation UnpitchedRelease, excluded ({' '.join(excluded)}), "
         f"min_held_ms {ep[52]}, max_held_ms {ep[53]}, keys {M(unp, 4)}, gain_scale 0.5 }}",
         # economy picking (engine_pars[10] = 3, L16196-16219): %tact___meta[64·a + 35]
         f"    picking {{ articulations ({' '.join(picked)}) }}",
         *([f"    tracks ({' '.join(f'{{pan {fmt(x)}}}' for x in pans)})"] if len(pans) > 1 else []),
         # x3 · meta[6]/10 µs (L7183)
         f"    start_offsets ({' '.join(f'{{articulation {a}, ms {fmt(ms)}}}' for a, ms in offsets)})",
         # extra attack (L18182-18203): %s3___rr_seed[8192 + …] = the 4-RR
         # order, change_tune (string − 3)·−2000 past string idx 3
         *(["    extra_pick { articulation ExtraPick, rr 4, cents_per_string -20, from_string 3 }"]
           if ep[84] > 0 else []),  # Extra Attack
         ]
    L.append("    rr_orders (")
    for i in range(7):
        L.append(f"        {{ rr {i + 2}, order ({' '.join(map(str, rr_seed[4096 * i: 4096 * (i + 1)]))}) }}")
    L += ["    )", "}", "articulations ("]

    for a, aid in enumerate(names):
        tab = table[512 * a: 512 * a + 128]
        edges = [v for v in range(2, 128) if tab[v] != tab[v - 1]]
        g = group(a)
        crv = float(g["ahdsr_AHDSRc"].split("/")[5])
        dec, sus = X(a, 32), X(a, 33)
        if M(a, 28) == 0:  # tightness (L7194-7240)
            arg = 1000 - X(a, 28)
            t = arg * (M(a, 18) - M(a, 17)) // 1000 + M(a, 17)
            dec, sus = min(t, dec), min(arg * 1000, sus)
        segs = env(X(a, 31), crv, dec, sus)
        L += ["    {",
              f"        id {aid}",
              f"        vel_thresholds ({' '.join(map(str, edges))})",
              f"        // %vel_db by Vel->Vol {X(a, 27)}",
              f"        vel_gain_db ({' '.join(vel_law(vel_db, curve, X(a, 27)))})",
              f"        // group {g['name']}",
              f"        level_db {fmt(float(g['vol_db']))}",
              f"        amp_env ( {env_line(segs)} )",
              "        amp_env_hold true",
              f"        release_ms_by_held ((0 {fmt(stage_ms(X(a, 34), 12501.0))}))"]
        L += group_fx(slots, gjson[int(g["idx"])]) + ["    }"]
    # side recordings outside TACT: their group's saved envelope
    for aid, word in ((("ExtraPick", "Extra Pick"),) if ep[84] > 0 else ()):
        g = gname(word)
        att, _, dec, sus, rel, crv = (float(x) for x in g["ahdsr_AHDSRc"].split("/"))
        lag = min(LAG_MS, att)
        segs = [(lag, 0.0, 0.5), (att - lag, 1.0, tension(attack_k(crv)))]
        if sus < 1:
            segs.append((dec, sus, tension(FALL_K)))
        L += ["    {", f"        id {aid}",
              f"        level_db {fmt(float(g['vol_db']))}",
              f"        amp_env ( {env_line(segs)} )",
              "        amp_env_hold true",
              f"        release_ms_by_held ((0 {fmt(rel)}))"]
        L += group_fx(slots, gjson[int(g["idx"])]) + ["    }"]
    L.append(")")
    path = os.path.join(root, "_patches", inst, "section.styx")
    if os.path.exists(path):
        import shutil, time
        shutil.copy(path, f"{path}.pre-{time.strftime('%Y-%m-%d-%H%M%S')}")
    open(path, "w").write("\n".join(L) + "\n")
    print(path)
