#!/usr/bin/env python3
"""Derive a Cinematic Studio library spec from the Kontakt-matched strings spec.

    python3 derive_cs_spec.py woodwinds > specs/cinematic-woodwinds.styx

CSS, CSW and CSB run one KSP engine family over the same group layout, with
each library's recordings in the strings' slots (CSW: Flutter in the
"harmonics" groups, Repetitions in "spiccato", NVsus in "nonvib"). So the
engine model the strings were matched with — performance, dynamics, the
articulations' engine settings, legato_engine, short_note_timing — carries
over, renamed to the library's articulations, minus the ones it has no
recordings of. The library's own identity — name, sections, mics, the
keyswitch layout — comes from its existing spec (`--own`, default
specs/cinematic-<library>.styx as committed, read from git HEAD so a
re-derive never feeds on its own output).

Per-instrument numbers (velocity tables, group curves, legato and marcato
knobs) come from each instrument's decoded KSP values in its
`_patches/<instrument>/section.styx`, not from here.
"""
import os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SPECS = os.path.join(HERE, "..", "specs")
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))

LIBS = {
    "woodwinds": {
        "title": "Cinematic Studio Woodwinds",
        # strings articulation -> this library's (whole word)
        "rename": {
            "Harmrel": "Fluttrel",
            "Harm": "Flutter",
            "Vsusrel": "Vibsusrel",
            "NVrel": "NVsusrel",
            "Nonvib": "NVsus",
            "Spiccato": "Repetitions",
        },
        # strings articulations this library has no recordings of
        "drop": ["Tremolo", "Tremrel", "Clegno", "Port", "Pizzicato", "Bartokpizz"],
        # top-level blocks of the strings spec with no counterpart here
        "drop_blocks": [],
        # library-wide differences from the strings engine (cs_ksp_var_map.tsv):
        # (regex, replacement, why) applied to the derived text
        "subs": [
            (r"(\n        id            Marcato\n(?:.*\n)*?\s+lt_articulation )Leg\b", r"\1MLeg",
             "CSW records its marcato transitions as their own MLeg files"),
            (r"(sustain_noteoff_ms\s+\S+[^\n]*)", r"\1\n    sustain_noteoff_cut_ms 100   // $tukcw — the script's fade_out over the body's key-up release",
         "CSW's script fades a released body to silence over $tukcw = 100 ms", "all"),
        (r"master_tune_cents\s+\S+[^\n]*", "master_tune_cents    0  // group tunes differ: folded into each zone (fold_group_tune.py)",
         "group tune is per group here, folded into the zones"),
        (r"\n\s+rr_sequence\s[^\n]*", "",
         "shorts draw their RR at random (random(1,n) avoiding the key's last picks), no sequence",
         "all"),
        (r"(\n        id            NVsus\n)",
         "\\1        script_start_ms 333   // $0kimq := 333 on a nonvib sustain; zones without a start-mod range ignore it\n",
         "a nonvib sustain starts 333 ms in where the zone lets it"),
        (r"(\n    lt_offset_low_latency[^\n]*)",
         "\\1\n    overlap_offset_zone1_only true  // CSW: $1fvjk := base + $b0n3s in velocity zone 1 only (9463-9478)",
         "the overlap delay deepens the transition entry in zone 1 only"),
        (r"(\n    vibrato_controller    CC2)\n",
         "\\1\n    // CC2 switches nonvib / vib at 64 (KSP `$takdp<64`, CC `$vkgua` = 2), with a\n"
         "    // timed crossfade (`$2b2p5`, 650 ms; not modelled yet): no CC2 group curves\n"
         "    vibrato_mode          \"on_off\"\n",
         "CSW's vibrato is a CC2 switch, not a group-curve crossfade"),
        (r"\n    first_note_variants[^\n]*", "",
             "no first-note body-variant counter ($2ekvq absent)"),
            (r"\n    first_note_variant_init[^\n]*", "", "(same)"),
            (r"\n    first_note_mute_ms[^\n]*", "",
             "no first-note mute path ($otlef / $yqhyd absent)"),
            (r"velocities \(\(5 7\) \(8 9\)\)", "velocities ((5 7) (5 7))",
             "the vibsus attack zones are 5-7 only (Repetitions); zone 3 UNVERIFIED"),
        ],
    },
}

LIBS["brass"] = {
    "title": "Cinematic Studio Brass",
    # strings slot -> brass recordings (csb_spec_requirements.md §1)
    "rename": {
        "Vsusrel": "Susrel",
        "Vibsus": "Sus",
        "Tremrel": "Mutesusrel",
        "Tremolo": "Mutesus",
        "Pizzicato": "Mutereps",
        "Clegno": "Rips",
        "Harmrel": "Fluttrel",
        "Harm": "Flutter",
        "Spiccato": "Repetitions",
    },
    # no vibrato dimension, no portamento, no bartok slot
    "drop": ["Nonvib", "NVrel", "NVLeg", "NVLegzero", "Port", "Bartokpizz"],
    "drop_blocks": [],
    # brass articulations with no strings slot, kept from the brass spec
    "own_articulations": ["Stoppedsus", "Stoppedreps", "Stoppedsusrel", "Taka"],
    "subs": [
        (r"(sustain_noteoff_ms\s+\S+[^\n]*)", r"\1\n    sustain_noteoff_cut_ms 200   // $qxeb4 — the script's fade_out over the body's key-up release",
         "CSB's script fades a released body to silence over $qxeb4 = 200 ms", "all"),
        (r"master_tune_cents\s+\S+[^\n]*", "master_tune_cents    0  // group tunes differ: folded into each zone (fold_group_tune.py)",
         "group tune is per group here, folded into the zones"),
        (r"\n\s+rr_sequence\s[^\n]*", "",
         "shorts draw their RR at random (random(1,n) avoiding the key's last picks), no sequence",
         "all"),
        (r"(\n        id            Marcato\n(?:.*\n)*?\s+rr_per_note\s+)true[^\n]*",
         "\\1false  // one RR toggle for every note ($d3x3j := 1 - $d3x3j, 11021 / 14060)",
         "CSB marcato alternates its two RRs globally, not per note-class"),
        (r"random_tune_cents\s+\d+[^\n]*",
         "random_tune_cents   0     // CSB detunes no short ($5xze0 = 0 on every patch; KSP 12794)",
         "no random detune on brass shorts", "all"),
        (r"\n    vibrato_controller[^\n]*",
         "\n    // The script ignores MIDI CC2 (it is in %vn532) and sets it to its saved\n"
         "    // vibrato knob ($takdp, 126 on every CSB patch): the release groups' CC2\n"
         "    // curves read that, so brass has no vibrato crossfade and a fixed CC2.\n"
         "    fixed_ccs ({cc 2, value 126})",
         "no vibrato crossfade; CC2 held at $takdp"),
        (r"\n(\s*//[^\n]*\n)*\s*short_note_cc1_map \{[^}]*\}", "",
         "the KSP never reads CC1 for the short type (CC58 / KS velocity) — verify in Kontakt"),
        (r"\n    first_note_variants[^\n]*", "", "no first-note body variants (Sus zones vel 1-4, one start)"),
        (r"\n    first_note_variant_init[^\n]*", "", "(same)"),
        (r"\n    first_note_attack \{[^\n]*", "", "no attack-voice zones in the vibsus groups"),
        (r"first_note_mute_ms[^\n]*", "first_note_mute_ms      (0 0)", "no mute wait; zones 2-3 fade in"),
        (r'(\n\s+)0-10  (?:Sustain|"Sus")', r'\g<1>0-10  "Sus"', "CC58 0-10 = sustain"),
        (r"overlay \{ articulation Repetitions, cc1_scale \([^)]*\) \}",
         "overlay { articulation Repetitions, cc1_scale (" + " ".join(["100"] * 128)
         + "), skip_after_hard_transition false, connected_from_velocity 64 }",
         "CSB overlay: no Velocomp, every first note, loud connected notes ($evk4m 64)"),
    ],
}

TAKE_FROM_STRINGS = ["performance", "dynamics", "articulations", "legato_engine", "short_note_timing"]
TAKE_FROM_OWN = ["sections", "mics", "keyswitch"]


def top_blocks(text):
    """{name: (start, end)} line spans of the top-level `name {`/`name (` blocks."""
    lines = text.split("\n")
    out = {}
    i = 0
    while i < len(lines):
        m = re.match(r"^([a-z_]+) [({]$", lines[i])
        if m:
            close = ")" if lines[i].endswith("(") else "}"
            j = i + 1
            while lines[j] != close:
                j += 1
            out[m.group(1)] = (i, j)
            i = j
        i += 1
    return out


def block(text, name):
    lines = text.split("\n")
    a, b = top_blocks(text)[name]
    # the comment lines directly above belong to the block
    while a > 0 and lines[a - 1].startswith("//"):
        a -= 1
    return "\n".join(lines[a : b + 1])


def scalar(text, key):
    m = re.search(rf"^{key}\s+(.*)$", text, re.M)
    return m.group(1) if m else None


def drop_articulations(arts, drop):
    """Remove the `    {` … `    }` entries whose id is in `drop`."""
    lines = arts.split("\n")
    out, i = [], 0
    while i < len(lines):
        if lines[i] == "    {":
            j = i
            while lines[j] != "    }":
                j += 1
            entry = lines[i : j + 1]
            ids = [re.match(r"^\s+id\s+(\S+)", l) for l in entry]
            aid = next((m.group(1) for m in ids if m), None)
            if aid not in drop:
                out.extend(entry)
            i = j + 1
            continue
        out.append(lines[i])
        i += 1
    return "\n".join(out)


def own_articulation_entries(own, ids):
    """The `    {` … `    }` articulation entries of the library's own spec
    with these ids, verbatim."""
    if not ids:
        return ""
    arts = block(own, "articulations").split("\n")
    out, i = [], 0
    while i < len(arts):
        if arts[i] == "    {":
            j = i
            while arts[j] != "    }":
                j += 1
            entry = arts[i : j + 1]
            m = next((re.match(r"^\s+id\s+(\S+)", l) for l in entry if re.match(r"^\s+id\s+", l)), None)
            if m and m.group(1) in ids:
                out.extend(entry)
            i = j + 1
            continue
        i += 1
    return "\n".join(out) + "\n" if out else ""


def rename(text, mapping):
    for old in sorted(mapping, key=len, reverse=True):
        text = re.sub(rf"(?<![A-Za-z]){re.escape(old)}(?![A-Za-z])", mapping[old], text)
    return text


def drop_refs(text, drop):
    """Blank fields that point at a dropped articulation (release_artic Tremrel)."""
    for d in drop:
        text = re.sub(rf"^(\s*)(release_artic|vibrato_pair|sordino_pair)\s+{d}\s*$", r'\1\2 ""', text, flags=re.M)
    return text


def main():
    lib = sys.argv[1]
    cfg = LIBS[lib]
    strings = open(os.path.join(SPECS, "cinematic-strings.styx")).read()
    own_rel = f"features/rigs/orchestra/specs/cinematic-{lib}.styx"
    own = subprocess.run(
        ["git", "-C", REPO, "show", f"HEAD:{own_rel}"], check=True, capture_output=True, text=True
    ).stdout

    parts = [
        f"// {cfg['title']} — library spec, DERIVED by tests/derive_cs_spec.py from the",
        "// Kontakt-matched cinematic-strings.styx (one KSP engine family, the same group",
        "// layout) — do not edit the engine-model blocks here; change the strings spec or",
        "// the derivation. Identity blocks (sections, mics, keyswitch) are this library's own.",
        "// Per-instrument numbers live in each instrument's _patches/<instrument>/section.styx.",
        "",
        f"name    {scalar(own, 'name')}",
        f"vendor  {scalar(own, 'vendor')}",
    ]
    for k in ("live_legato_interval_max", "live_chord_window_ms"):
        v = scalar(strings, k)
        if v is not None:
            parts.append(f"{k} {v}")
    parts.append("")
    for name in ["sections", "mics"]:
        parts += [block(own, name), ""]
    for name in TAKE_FROM_STRINGS:
        if name in cfg["drop_blocks"]:
            continue
        b = block(strings, name)
        if name == "articulations":
            b = drop_articulations(b, set(cfg["drop"]))
            extra = own_articulation_entries(own, cfg.get("own_articulations", []))
            if extra:
                b = b.rstrip()
                assert b.endswith(")")
                b = b[:-1] + extra + ")"
        b = drop_refs(rename(b, cfg["rename"]), cfg["drop"])
        if name == "dynamics":
            # the pizzicato type map has no pizzicato to pick
            b = re.sub(r"\n(\s*//[^\n]*\n)*\s*pizzicato_cc1_map \{[^}]*\}\n", "\n", b)
        parts += [b, ""]
    parts += [block(own, "keyswitch"), ""]
    text = "\n".join(parts)
    for sub in cfg.get("subs", []):
        pat, rep, why = sub[:3]
        # a 4th element "all" applies the rule everywhere, not just once
        text, n = re.subn(pat, rep, text, count=0 if sub[3:] == ("all",) else 1)
        if n == 0:
            raise SystemExit(f"derive: substitution not applied ({why}): {pat}")
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
