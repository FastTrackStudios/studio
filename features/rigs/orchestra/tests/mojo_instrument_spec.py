#!/usr/bin/env python3
"""MOJO 2 Horn Section: one instrument's shipped values → `_patches/<instrument>/section.styx`.

usage: mojo_instrument_spec.py <library root> [<NKI name> …]   (default: every full instrument)

Reads `_ksp/<NKI name>/{persistent_1.tsv, groups.json, program.json}`; line
numbers are script_1.ksp (identical in every full instrument). Per
articulation (script order, `%ArtGroupStart` = persistent `%hr4to`):

- vel_gain_db: what the script sets the played groups' ENGINE_PAR_VOLUME to
  after `play_note` in velocity mode (33421-33431):
  `VelGainMin + (knob − VelGainMin)·(0.99^(v−1) − 1)/(0.99^126 − 1)`, with
  knob = the articulation's volume control (`%aal4d[art]`, read from its
  persistent value) and VelGainMin = `$hcxoq`; engine units → dB by the
  script's own law `(18000·log2(ep) − 346768.23)/1000` (25510).
- level_db: `change_vol(−$rvlgn · players)` per note (25480), 1 player.
- script_start_ms: `play_note`'s offset `$wv2n2 + $a0xhd` ms (25232),
  limited by each zone's start_mod (the engine does that).
- key_range: `%ArtRange` (persistent `%yqd5p`): outside it nothing plays (25170).
- rr / rr_random_avoid_last: `$1f1tu` round robins; RR mode `$axg22` = 1 →
  random, never the last pick (20460).
- amp_env: the articulation's Bus1 group volume ENV_AHDSR as flex segments
  — attack to 1 (held while the key is down), then the release to 0
  (`amp_env_release 1`). Hold/decay are 0 and sustain 1 on every shipped
  one-shot group; anything else is reported, not guessed.
"""
import csv, json, math, os, re, shutil, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mojo_zones  # noqa: E402  (the group classifier)

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/run/media/AudioHaven/Sampled/Horns/MOJO 2 Horn Section"

# Script articulation order (185-197) → spec id, its volume knob variable
# (`%aal4d[i] := get_ui_id(<knob>)`, 3275-8852) and whether it is modelled.
ARTS = [
    ("Sus", "$tnc12"), ("Stacc", "$gndi4"), ("Stabs", "$423xn"), ("BendDown", "$chdav"),
    ("RunDown", "$3hz2h"), ("RunUp", "$cekgd"), ("Doits", "$oijh1"), ("Rise", "$hbhgf"),
    ("Shakes", "$jzee2"), ("Trills", "$egrpi"), ("Swells", "$rekbd"), ("Cresc", "$yoa4p"),
    ("Falls", "$5naxv"),
]
MODELLED = {"Sus", "Stacc", "Stabs", "BendDown", "RunDown", "RunUp", "Doits", "Rise", "Shakes", "Trills", "Falls"}


def persistent(path):
    out = {}
    for row in csv.reader(open(path), delimiter="\t"):
        if len(row) >= 2 and row[0] != "name":
            out[row[0]] = row[1].split()
    return out


def ep_db(ep):
    """Kontakt engine volume units → dB (the script's own conversion)."""
    return (18000.0 * math.log2(ep) - 346768.2342478351) / 1000.0


def vel_table(knob, vmin):
    den = 0.99 ** 126 - 1.0
    out = []
    for v in range(128):
        vv = max(v, 1)
        ep = vmin + int((knob - vmin) * ((0.99 ** (vv - 1) - 1.0) / den))  # real_to_int truncates
        out.append(round(ep_db(ep), 4))
    return out


def ahdsr_env(groups, start):
    """The articulation's Bus1 group volume AHDSR → flex segments, or None."""
    g = next((g for g in groups if g["index"] == start), None)
    env = g and next((e for e in g["envelopes"] if e["kind"] == "ENV_AHDSR" and e["target"] == "volume"
                      and not e["bypassed"]), None)
    if not env:
        return None, f"group {start}: no volume AHDSR"
    if env["hold_ms"] or env["decay_ms"] or abs(env["sustain"] - 1.0) > 1e-6 or env["ahd_only"]:
        return None, (f"group {start}: AHDSR H {env['hold_ms']} D {env['decay_ms']} S {env['sustain']} "
                      f"ahd_only {env['ahd_only']} — not a plain attack/release")
    # curve 0 on every shipped one-shot group: a linear attack (tension 0.5).
    # Kontakt's AHDSR release is curved; its shape over 9950 ms is measured,
    # not decoded — linear here until the null test says otherwise.
    tension = 0.5 - 0.5 * env["curve"]
    return [(max(env["attack_ms"], 0.0), 1.0, tension), (env["release_ms"], 0.0, 0.5)], None


def group_eq(art, start, names, fx_slots, notes, lp_floor):
    """`(dynamic, stages)` per layer of `art`: the active inserts of each
    layer's Bus1 group (the group the shipped Mix mic plays), as spec
    stages `(kind, params)` in slot order. Layers are the articulation's
    4-group blocks from `start` (dynamic named by mojo_zones.classify).

    - kind 7, filter type 22-24: the EQ;
    - kind 7, filter type 50: the one-layer velocity lowpass. The script
      sets its cutoff to min(1, v/127 + $SimDynMinLP/1e6) (33439-33452):
      a 0 cutoff plus a velocity route with that table;
    - kind 9: a stereo width (its parameter, the float at byte 9: side ×
      (1 + p), measured on Trumpet Stabs). Its output gain is on the zones
      (mojo_zones.py).
    Anything else is noted, not run."""
    out, k = [], 0
    while True:
        gi = start + 4 * k
        cls = mojo_zones.classify(names.get(gi, "")) if gi in names else None
        if cls is None or cls[0] != art:
            break
        dyn = cls[1]
        stages = []
        for s in sorted(fx_slots.get(gi, []), key=lambda s: s["slot"]):
            if s.get("bypass", True):
                continue
            if s["kind"] == 9:
                _, width = mojo_zones.group_stage({0: [s]}, 0)
                if width:
                    stages.append(("width", {"width": width}))
            elif s["kind"] == 7 and s.get("filter_type") in (22, 23, 24) and s.get("eq_bands"):
                stages.append(("eq", {"bands": [v for b in s["eq_bands"] for v in b]}))
            elif s["kind"] == 7 and s.get("filter_type") == 50:
                table = [min(1.0, v / 127.0 + lp_floor) for v in range(128)]
                stages.append(("sv_lp1", {"cutoff": 0.0, "resonance": float(s.get("resonance") or 0.0),
                                          "velocity_table": table}))
            else:
                notes.append(f"{art} {dyn}: insert kind {s['kind']} type {s.get('filter_type')} "
                             f"(slot {s['slot']}) not run")
        if stages:
            out.append((dyn, stages))
        k += 1
    return out


def release_numbers(ksp, pers):
    """Normal release (offset ms, fade ms): script_0 hands them over at load
    (persistent_0 `$RelNormOffset` / `$RelNormFade`); persistent_1's copies
    (`%ej4i2` / `%eg214`) can be cut short (Alto)."""
    p0 = {}
    path = os.path.join(ksp, "persistent_0.tsv")
    if os.path.exists(path):
        p0 = persistent(path)
    off = p0.get("$RelNormOffset", [pers["%ej4i2"][0]])[0]
    fade = p0.get("$RelNormFade", [pers["%eg214"][0]])[0]
    return float(off), float(fade)


def release_gain_table(rel_knob, sus_knob, vmin, trim_db):
    """The normal release sample's level by the note-on's raw velocity, dB
    (release callback 36892-36970): the release groups at 630000 (0 dB),
    + dB(release volume knob) + dB(sustain volume knob)
    + dB(VelGainMin + ⌊v·(630000 − VelGainMin)/127⌋) − the per-note trim."""
    out = []
    for v in range(128):
        ep = vmin + (v * (630000 - vmin)) // 127
        out.append(round(ep_db(630000) + ep_db(rel_knob) + ep_db(sus_knob) + ep_db(max(ep, 1)) + trim_db, 4))
    return out


def fmt(v):
    s = f"{v:.4f}".rstrip("0").rstrip(".")
    return s if s not in ("-0", "") else "0"


def build(nki):
    ksp = os.path.join(ROOT, "_ksp", nki)
    inst = re.sub(r"\s+2\.0$", "", nki)
    pers = persistent(os.path.join(ksp, "persistent_1.tsv"))
    groups = json.load(open(os.path.join(ksp, "groups.json")))
    program = json.load(open(os.path.join(ksp, "program.json")))
    one = lambda k: int(pers[k][0])
    starts = [int(x) for x in pers["%hr4to"]]
    ranges = [int(x) for x in pers["%yqd5p"]]
    num_rr, vmin = one("$1f1tu"), one("$hcxoq")
    if one("$xhoj1") != 2:
        raise SystemExit(f"{nki}: velocity curve {one('$xhoj1')} is not LINEAR — thresholds would move")
    rr_avoid = 1 if one("$axg22") == 1 and one("$mrsqg") == 0 else 0
    start_ms = one("$wv2n2") + one("$a0xhd")
    level_db = -one("$rvlgn") / 1000.0  # × 1 player

    buses = json.load(open(os.path.join(ksp, "buses.json")))
    bus1 = next(b for b in buses if b["index"] == 0)
    bus1_vol = bus1["pub"][0]
    gain_db = 20 * math.log10(program["volume"]) + 20 * math.log10(bus1_vol)
    fx_slots = {x["group"]: x["slots"] for x in json.load(open(os.path.join(ksp, "group_fx.json")))}
    names = {g["index"]: g["name"] for g in groups}

    lines = [
        f"// Generated by mojo_instrument_spec.py from MOJO 2 {nki} — do not edit.",
        f"// Instrument gain = program volume {program['volume']} × Bus 1 (the Mix mic's bus)",
        f"// fader {bus1_vol}. Not modelled yet: the active instrument insert (slot 7,",
        "// kind 11), the one-layer articulations' velocity lowpass (slot 7, type 50).",
        f"instrument_gain_db {fmt(gain_db)}",
        "articulations (",
    ]
    notes = []
    for i, (art, knob_var) in enumerate(ARTS):
        if art not in MODELLED:
            continue
        knob = one(knob_var)
        env, why = ahdsr_env(groups, starts[i])
        if why:
            notes.append(f"{art}: {why}")
        fx = group_eq(art, starts[i], names, fx_slots, notes, one("$lrshn") / 1e6)
        table = vel_table(knob, vmin)
        lines += [
            "    {",
            f"        id {art}",
            f"        rr {num_rr}",
            f"        rr_random_avoid_last {rr_avoid}",
            f"        script_start_ms {start_ms}",
            f"        level_db {fmt(level_db)}",
            f"        key_range ({ranges[2 * i]} {ranges[2 * i + 1]})",
            f"        // knob {knob} → {fmt(ep_db(knob))} dB at velocity 127; VelGainMin {vmin} → {fmt(ep_db(vmin))} dB",
            f"        vel_gain_db ({' '.join(fmt(x) for x in table)})",
        ]
        if env:
            body = " ".join(f"{{time_ms {fmt(t)}, level {fmt(lv)}, curve {fmt(c)}}}" for t, lv, c in env)
            lines += [f"        amp_env ( {body} )", "        amp_env_hold true", "        amp_env_release 1"]
        if art == "Sus":
            off, fade = release_numbers(ksp, pers)
            gains = release_gain_table(one("$ues0b"), knob, vmin, level_db)
            lines += [
                "        // the release callback (36445-37013): body fade_out + the normal",
                "        // release sample; each new sustain note fades sounding releases (25218)",
                "        fade_releases_ms 75",
                f"        release_sample {{ artic SusRel, start_ms {fmt(off)}, fade_ms {fmt(fade)},",
                f"            gain_db ({' '.join(fmt(x) for x in gains)}) }}",
            ]
        if fx:
            lines.append("        group_fx (")
            for dyn, stages in fx:
                st, rt = [], []
                for n, (kind, p) in enumerate(stages):
                    if kind == "eq":
                        st.append(f"{{ kind eq, bands ({' '.join(fmt(v) for v in p['bands'])}) }}")
                    elif kind == "width":
                        st.append(f"{{ kind width, width {fmt(p['width'])} }}")
                    else:
                        st.append(f"{{ kind {kind}, cutoff {fmt(p['cutoff'])}, resonance {fmt(p['resonance'])} }}")
                        if p.get("velocity_table"):
                            rt.append(f"{{ stage {n}, param cutoff, cc 0, source velocity, "
                                      f"table ({' '.join(fmt(v) for v in p['velocity_table'])}) }}")
                lines.append(f"            {{ dynamic {dyn}, stages ({' '.join(st)}), routes ({' '.join(rt)}) }}")
            lines.append("        )")
        lines.append("    }")
    # The normal release articulation (`$MiscLongNVNormalRelGroupStart`): its
    # zones' RR, the release groups' inserts, and its envelope — fade_in over
    # RelFade from the note-off, then fade_out over 2^(SusRel/73477+1) − 2 ms.
    off, fade = release_numbers(ksp, pers)
    tail_ms = 2 ** (one("$q1stw") / 73477 + 1) - 2
    rel_start = int(persistent(os.path.join(ksp, "persistent_0.tsv")).get(
        "$MiscLongNVNormalRelGroupStart", [one("$ywa0p")])[0])
    rel_fx = group_eq("SusRel", rel_start, names, fx_slots, notes, one("$lrshn") / 1e6)
    lines += [
        "    {",
        "        id SusRel",
        f"        rr {num_rr}",
        f"        rr_random_avoid_last {rr_avoid}",
        f"        amp_env ( {{time_ms {fmt(fade)}, level 1, curve 0.5}} {{time_ms {fmt(tail_ms)}, level 0, curve 0.5}} )",
        "        amp_env_hold false",
    ]
    if rel_fx:
        lines.append("        group_fx (")
        for dyn, stages in rel_fx:
            st = []
            for kind, p in stages:
                if kind == "eq":
                    st.append(f"{{ kind eq, bands ({' '.join(fmt(v) for v in p['bands'])}) }}")
                elif kind == "width":
                    st.append(f"{{ kind width, width {fmt(p['width'])} }}")
            lines.append(f"            {{ dynamic {dyn}, stages ({' '.join(st)}), routes () }}")
        lines.append("        )")
    lines.append("    }")
    lines.append(")")

    patch = os.path.join(ROOT, "_patches", inst)
    os.makedirs(patch, exist_ok=True)
    dst = os.path.join(patch, "section.styx")
    new = "\n".join(lines) + "\n"
    if os.path.exists(dst):
        if open(dst).read() == new:
            print(f"{inst}: unchanged")
            return
        shutil.copy2(dst, dst + time.strftime(".pre-%Y-%m-%d-%H%M%S"))
    open(dst, "w").write(new)
    print(f"{inst}: RR {num_rr}, start {start_ms} ms, trim {fmt(level_db)} dB"
          + ("".join(f"\n    note: {n}" for n in notes)))


def main():
    names = sys.argv[2:] or sorted(
        d for d in os.listdir(os.path.join(ROOT, "_ksp")) if " - " not in d and d.endswith("2.0")
    )
    for n in names:
        build(n)


if __name__ == "__main__":
    main()
