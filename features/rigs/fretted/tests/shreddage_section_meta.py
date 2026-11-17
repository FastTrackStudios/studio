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
    timer_ms = int(re.search(r"set_listener\(\$NI_SIGNAL_TIMER_MS,(\d+)\)", script).group(1)) / 1000
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
    # TACT rules (x23..x25: 0 KEY x11, 1 VEL x13..x14): an articulation on
    # another's keyswitch with a VEL rule is a velocity split of the one
    # whose range takes velocity 1
    rules = lambda a: [X(a, 23 + i) for i in range(3)]
    keyed = [a for a in range(na) if 0 in rules(a) and 1 in rules(a)]
    splits = []
    for a in keyed:
        base = [b for b in keyed if X(b, 11) == X(a, 11) and X(b, 13) == 1]
        if X(a, 13) > 1 and base:
            splits.append((names[base[0]], X(a, 13), X(a, 14), names[a]))
    # trigger type 0 (the rake): its noise, then after engine_pars[54]·100
    # + 10 µs the Sustain (artic 0, L17298-17343) at the rake's Vel->Vol;
    # past the strings of L16193 it is the Sustain plain
    leads = []
    for a in range(na):
        if M(a, 21) != 0:
            continue
        m = re.search(rf"if \(\$ncb__sel_artic={a} and \(\$ncb__string=\d+ or \(\$ncb__string>(\d+)\)", script)
        sel = re.search(r"if \(%s3__engine_pars\[55\]=(\d+)\)\n\$math__rand_seed", script)
        if not sel or int(sel.group(1)) != ep[55]:
            sys.exit("trigger 0 with a fixed RR (engine_pars[55]): not modelled")
        leads.append(f"{{ articulation {names[a]}, min_velocity 0, lead {names[a]}, body {names[0]}"
                     + (f", max_string {m.group(1)}" if m else "")
                     + f", body_delay_ms {fmt((ep[54] * 100 + 10) / 1000)}, body_offset_ms 0"
                     + ", draw_modulus 10, body_lead_gain true }")
    # multitrack jitter (L7209-7229, L7557-7570): random(lo, hi) per track
    # for tune and offset; the offset wraps at meta[6]/10 · 1000 µs
    jit = [names[a] for a in ksp_list(script, "s3__no_neighbor_borrowing_artics")]
    wraps = {M(a, 6) // 10 * 1000 for a in range(na)}
    if len(wraps) != 1:
        sys.exit(f"offset ranges {wraps}: one jitter wrap only")
    jr = re.search(r"case 1\n\$_trigger__tune := \$_trigger__tune_init\+random\((-?\d+),(-?\d+)\)\n"
                   r"[^\n]*\ncase 2\n\$_trigger__tune := \$_trigger__tune_init\+random\((-?\d+),(-?\d+)\)\n"
                   r"[^\n]*\ncase 3\n\$_trigger__tune := \$_trigger__tune_init\+random\((-?\d+),(-?\d+)\)", script)
    ranges = [(0, 0)] + [(int(jr.group(i)), int(jr.group(i + 1))) for i in (1, 3, 5)]
    all_tracks = [names[a] for a in range(na) if M(a, 21) in (3, 4)]

    # stitched slides (engine_pars[31] slide speed, L16695-16701)
    sl = ksp_list(script, "s3__slide_artics")[0]
    scale = [float(x) for x in re.search(r"declare \?sti__wait_scale_lookup\[\d+\] := \(([^)]*)\)",
                                         script).group(1).split(",")]
    max_iv = int(re.search(r"abs\(\$_distance\)>0 and \(\$_slide_type # -1 and \(abs\(\$_distance\)<=(\d+)\)",
                           script).group(1))
    inner = int((ep[31] + 1000) * -3500 / 2000) + 5000  # KSP integer division truncates

    def step(d):
        w = int(float(inner // d * 64) * scale[d - 1])
        wc = min(max(w, 9000), 70000)
        wf = int(((wc - 9000.0) * -1.9 / 61000.0 + 2.0) * 100.0)
        vol = -2000 + int(9000.0 * (10 / 100.0 * (wf / 100.0)) + 9000.0 * 10 / 100.0)
        fade = int(35571.0 * (1.0 + w / 500000.0))
        return f"{{wait_us {w}, gain_db {fmt(vol / 1000)}, dest_fade_in_us {fade}}}"

    lg = lambda v: 18000.0 * math.log2(v) - 346768.2342478351
    lo = int(2.0 ** 15.931568569324172)
    sl_vol = min(max(X(sl, 2), lo), 1000000)
    cached = int(p["$s3__cached_slide_volume"][0])
    squeak_mdb = min(max(int(lg(sl_vol) - lg(cached)), -60000), 12000)
    dest_mdb = int(lg(X(sl, 2) + 1))
    sq_keys = int(re.search(r"\$sti__slide_squeak__rr := \(%sti__slide_squeak___rr_counter\[[^\]]*\]\+\$s3__i\) mod (\d+)",
                            script).group(1))
    rel_fade = int(re.search(r"\$ncb__slide_rel_fade := (\d+)", script).group(1))
    slide_from = [names[a] for a in range(na) if M(a, 9) == 1 and X(a, 5) == 1 and M(a, 21) != 0]
    # Argent's legato slide plays the destination on the last step (L16889);
    # Darkwall's keeps the last stitch as the destination (§6, L16433-16776)
    keep_last = "if ($sti__i=abs($sti__distance) and not" not in script

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
         # s3__engine__reset_hand from the timer listener once nothing is held
         # for engine_pars[59] ms (on release L18127, on listener L25632)
         f"    idle_reset {{ after_ms {ep[59]}, tick_ms {timer_ms} }}",
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
         # hammer (Abyss model §5; Argent/Darkwall §5): %s3__hammer_sources
         # ring, the new note's articulation in %s3__hammerable_artics, its
         # velocity within Legato (Hammer/Pull)'s VEL rule (matrix x14),
         # |interval| ≤ engine_pars[8]; volume_offset engine_pars[27]·−5000
         # mdB; engine_pars[28] forces the ringing string
         f"    hammer {{ articulation HammerPull, "
         f"sources ({' '.join(names[a] for a in lists['s3__hammer_sources'])}), "
         f"from ({' '.join(names[a] for a in lists['s3__hammerable_artics'])}), "
         f"max_velocity {X(names.index('HammerPull'), 14)}, range {ep[8]}, gain_db {-5 * ep[27]}, "
         f"force {'true' if ep[28] == 1 else 'false'} }}",
         # Tapping (trigger type 8): a hammer from the ringing note or the
         # fret; an idle open string plays Sustain
         "    tap { articulation Tapping, open_as Sustain }",
         *([f"    tracks ({' '.join(f'{{pan {fmt(x)}}}' for x in pans)})"] if len(pans) > 1 else []),
         # x3 · meta[6]/10 µs (L7183)
         f"    start_offsets ({' '.join(f'{{articulation {a}, ms {fmt(ms)}}}' for a, ms in offsets)})",
         # extra attack (L18182-18203): %s3___rr_seed[8192 + …] = the 4-RR
         # order, change_tune (string − 3)·−2000 past string idx 3
         *(["    extra_pick { articulation ExtraPick, rr 4, cents_per_string -20, from_string 3 }"]
           if ep[84] > 0 else []),  # Extra Attack
         *([f"    velocity_splits ({' '.join(f'{{articulation {b}, min_velocity {lo}, max_velocity {hi}, plays {a}}}' for b, lo, hi, a in splits)})"]
           if splits else []),
         *([f"    lead_ins ({' '.join(leads)})"] if leads else []),
         # the slide key (KS of %s3__slide_artics[0], held); sources
         # %s3__slidable_artics; from: meta[9] and matrix[5] set; the source
         # fades over $ncb__slide_rel_fade µs (L16312); a stitch per fret
         # (L16754-17167), the destination at 99 900 µs + its x3 offset,
         # clamped below meta[6]·100 (L16897); squeak (L16704-16745) at
         # 18000·log2(slide vol / cached slide vol) mdB + Sustain's Vel->Vol
         f"    slide {{ key {X(sl, 11)}, articulation {names[sl]}, "
         f"sources ({' '.join(names[a] for a in ksp_list(script, 's3__slidable_artics'))}), "
         f"from ({' '.join(slide_from)}), max_interval {max_iv}, time_us (), "
         f"source_fade_ms {fmt(rel_fade / 1000)}, transition_fade_in_ms 0, transition_fade_out_ms 0, "
         f"body_offset_ms 0, body_fade_in_ms 0, age_db_per_s {-1 * ep[29]}, stitched {{ "
         f"steps ({' '.join(step(d) for d in range(1, max_iv + 1))}), "
         f"dest_offset_ms 99.9, dest_offset_max_ms {fmt((M(0, 6) * 100 - 1) / 1000)}, "
         f"dest_gain_db {fmt(dest_mdb / 1000)}, squeak {{ articulation FretSqueak, keys {sq_keys}, "
         f"gain_db {fmt(squeak_mdb / 1000)}, vel_gain_from {names[0]} }}"
         f"{', keep_last true' if keep_last else ''} }} }}",
         *([f"    jitter {{ articulations ({' '.join(jit)}), "
            f"all_tracks ({' '.join(all_tracks)}), "
            f"ranges ({' '.join(f'{{lo {lo}, hi {hi}}}' for lo, hi in ranges)}), "
            f"offset_wrap_us {wraps.pop()} }}"] if len(pans) > 1 else []),
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
              # an AHDSR holds at its sustain; a sustain of 0 decays to silence
              f"        amp_env_hold {'true' if segs[-1][1] > 0.01 else 'false'}",
              # no release segment: a short voice would otherwise take its third
              # (the decay) as one and hold before it; the release is the
              # voice's own note-off fade (release_ms_by_held)
              f"        amp_env_release {len(segs)}",
              f"        release_ms_by_held ((0 {fmt(stage_ms(X(a, 34), 12501.0))}))"]
        # a second volume AHDSR on the group (Argent's Slide: 23.7 ms attack)
        # multiplies the script-driven one; its release is shorter-lived than
        # the first's, so it runs through
        vols = [e for e in gjson[int(g["idx"])]["envelopes"]
                if e["kind"] == "ENV_AHDSR" and e["target"] == "volume" and not e["bypassed"]]
        if len(vols) > 1:
            e = vols[1]
            lag = min(LAG_MS, e["attack_ms"])
            segs2 = [(lag, 0.0, 0.5), (e["attack_ms"] - lag, 1.0, tension(attack_k(e["curve"])))]
            if e["hold_ms"] > 0:
                segs2.append((e["hold_ms"], 1.0, 0.5))
            if e["sustain"] < 1:
                segs2.append((e["decay_ms"], e["sustain"], tension(FALL_K)))
            L.append(f"        amp_env2 ( {env_line(segs2)} )")
        L += group_fx(slots, gjson[int(g["idx"])]) + ["    }"]
    # side recordings outside TACT: their group's saved envelope
    side = [("FretSqueak", "Fret Squeaks Str 1")] + ([("ExtraPick", "Extra Pick")] if ep[84] > 0 else [])
    for aid, word in side:
        g = gname(word)
        att, _, dec, sus, rel, crv = (float(x) for x in g["ahdsr_AHDSRc"].split("/"))
        lag = min(LAG_MS, att)
        segs = [(lag, 0.0, 0.5), (att - lag, 1.0, tension(attack_k(crv)))]
        if sus < 1:
            segs.append((dec, sus, tension(FALL_K)))
        L += ["    {", f"        id {aid}",
              f"        level_db {fmt(float(g['vol_db']))}",
              f"        amp_env ( {env_line(segs)} )",
              # an AHDSR holds at its sustain; a sustain of 0 decays to silence
              f"        amp_env_hold {'true' if segs[-1][1] > 0.01 else 'false'}",
              # no release segment: a short voice would otherwise take its third
              # (the decay) as one and hold before it; the release is the
              # voice's own note-off fade (release_ms_by_held)
              f"        amp_env_release {len(segs)}",
              f"        release_ms_by_held ((0 {fmt(rel)}))"]
        L += group_fx(slots, gjson[int(g["idx"])]) + ["    }"]
    L.append(")")
    path = os.path.join(root, "_patches", inst, "section.styx")
    if os.path.exists(path):
        import shutil, time
        shutil.copy(path, f"{path}.pre-{time.strftime('%Y-%m-%d-%H%M%S')}")
    open(path, "w").write("\n".join(L) + "\n")
    print(path)
