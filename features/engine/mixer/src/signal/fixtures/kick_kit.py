"""The Kick folder's `signal.chain/1` fixture, from the kit's own states.

    python3 -I kick_kit.py /tmp/signal-kit > kick_kit.json

Each `.state` is an 8-byte little-endian length and a JSON document whose
`signal-state` field holds Signal's preset (its lanes, each lane's blocks
and phase trims). The chain a Signal instance answers for its port is
that lane: its blocks grouped by phase, each phase's trim as its make-up.
The ranges, units and clap ids are the fixture's own (the preset has
values, not parameter metadata) until the CLAP answers `signal/1`.
"""

import json
import struct
import sys
from pathlib import Path

KICK = [
    "Drums/Kick",
    "Drums/Kick/Sum/In",
    "Drums/Kick/Sum/Out",
    "Drums/Kick/Sum/Trig",
    "Drums/Kick/Sum/Sample",
    "Drums/Kick/Sub",
    "Drums/Kick/Verb/Room",
]

PHASES = ["source", "rescue", "tone", "polish", "relational", "depth"]
PHASE_NAMES = {p: p.capitalize() for p in PHASES}

KINDS = {"drive": "saturate", "drum": "sampler"}
NAMES = {
    "gate": "Gate",
    "comp": "Comp",
    "eq": "EQ",
    "saturate": "Saturate",
    "reverb": "Reverb",
    "sampler": "Sampler",
    "trigger": "Trigger",
}

# id: (name, unit, min, max, default, choices)
PARAMS = {
    "threshold": ("Threshold", "dB", -80.0, 0.0, -40.0, []),
    "length": ("Length", "ms", 10.0, 1000.0, 300.0, []),
    "mode": ("Mode", "", 0.0, 1.0, 0.0, ["Gate", "Duck"]),
    "ratio": ("Ratio", ":1", 1.0, 20.0, 4.0, []),
    "attack": ("Attack", "ms", 0.1, 100.0, 10.0, []),
    "release": ("Release", "ms", 10.0, 1000.0, 100.0, []),
    "drive": ("Drive", "dB", 0.0, 24.0, 0.0, []),
    "low_cut": ("Low Cut", "Hz", 20.0, 400.0, 20.0, []),
    "mix": ("Mix", "%", 0.0, 1.0, 1.0, []),
    "dynamics": ("Dynamics", "%", 0.0, 1.0, 0.5, []),
    "level": ("Level", "dB", -60.0, 12.0, 0.0, []),
    "velocity": ("Velocity", "", 1.0, 127.0, 100.0, []),
    "threshold_db": ("Threshold", "dB", -80.0, 0.0, -30.0, []),
    "output": ("Output", "dB", -24.0, 24.0, 0.0, []),
    "band.1.freq": ("Band 1 Freq", "Hz", 20.0, 20000.0, 1000.0, []),
    "band.1.q": ("Band 1 Q", "", 0.1, 10.0, 0.7, []),
    "band.1.shape": ("Band 1 Shape", "", 0.0, 8.0, 0.0, []),
    "band.1.slope": ("Band 1 Slope", "", 0.0, 4.0, 1.0, []),
    "band.1.used": ("Band 1 On", "", 0.0, 1.0, 0.0, ["Off", "On"]),
}

METERS = {
    "gate": [("gain_reduction", "GR", "dB")],
    "comp": [("gain_reduction", "GR", "dB")],
    "saturate": [("drive", "Drive", "dB")],
    "sampler": [("level", "Level", "dB")],
    "trigger": [("hit", "Hit", "")],
    "reverb": [("level", "Level", "dB")],
}


def text(value, unit, choices):
    if choices:
        return choices[min(len(choices) - 1, max(0, round(value)))]
    if unit == "dB":
        return f"{value:.1f} dB"
    if unit == "ms":
        return f"{value:.0f} ms" if value >= 10 else f"{value:.1f} ms"
    if unit == ":1":
        return f"{value:.1f}:1"
    if unit == "Hz":
        return f"{value / 1000:.2f} kHz" if value >= 1000 else f"{value:.0f} Hz"
    if unit == "%":
        return f"{value * 100:.0f}%"
    return f"{value:.2f}" if value % 1 else f"{value:.0f}"


def param(pid, value, clap_id):
    name, unit, lo, hi, default, choices = PARAMS[pid]
    return {
        "id": pid,
        "clap_id": clap_id,
        "name": name,
        "value": value,
        "normalized": round((value - lo) / (hi - lo), 4),
        "text": text(value, unit, choices),
        "unit": unit,
        "min": lo,
        "max": hi,
        "default": default,
        "choices": choices,
    }


def preset_of(path):
    raw = path.read_bytes()
    (n,) = struct.unpack("<Q", raw[:8])
    doc = json.loads(raw[8 : 8 + n])
    return json.loads(json.loads(doc["fields"]["signal-state"]))


def chain_of(state, revision):
    port = state["instance"]["port"]
    lane = next(l for l in state["preset"]["lanes"] if l["port"] == port)
    blocks = list(lane["blocks"])
    if "trigger" in lane:
        blocks.append(
            {
                "id": "trigger",
                "kind": {"signal": "trigger"},
                "params": dict(lane["trigger"]),
                "bypass": False,
                "phase": "source",
            }
        )
    trims = {t["phase"]: t["gain_db"] for t in lane.get("trims", [])}
    meter_index = 0
    phases = {}
    for bi, b in enumerate(blocks):
        kind = KINDS.get(b["kind"]["signal"], b["kind"]["signal"])
        phase = b.get("phase", "source")
        params = b.get("params") or {"output": 0.0}
        meters = []
        for mid, mname, munit in METERS.get(kind, []):
            meters.append({"id": mid, "name": mname, "unit": munit, "index": meter_index})
            meter_index += 1
        key = None
        if "key" in b:
            key = {"from": b["key"], "via": "hits"}
        phases.setdefault(phase, []).append(
            {
                "id": f"{phase}.{b['id']}",
                "kind": kind,
                "name": NAMES.get(kind, kind.capitalize()),
                "bypass": b.get("bypass", False),
                "algorithm": b.get("algorithm"),
                "backend": "linked",
                "status": "ok",
                "key": key,
                "params": [
                    param(pid, float(v), f"b{bi:02d}{pi}")
                    for pi, (pid, v) in enumerate(params.items())
                ],
                "meters": meters,
            }
        )
    midi = None
    if any(b["kind"]["signal"] == "drum" for b in blocks):
        midi = {
            "plays": [{"note": 36, "piece": "Kick", "block": "source.drum"}],
            "publishes_hits": False,
        }
    phase_meters = {}
    for p in PHASES:
        if p in phases and p != "source":
            phase_meters[p] = [
                {"id": "level", "name": f"{PHASE_NAMES[p]} level", "unit": "dB", "index": meter_index}
            ]
            meter_index += 1
    return {
        "schema": "signal.chain/1",
        "revision": revision,
        "session": state["instance"].get("session"),
        "port": port,
        "mode": "per-track",
        "midi": midi,
        "phases": [
            {
                "id": p,
                "name": PHASE_NAMES[p],
                "bypass": False,
                "makeup_db": round(trims.get(p, 0.0), 2),
                "blocks": phases[p],
                "meters": phase_meters.get(p, []),
            }
            for p in PHASES
            if p in phases
        ],
        "macros": [],
    }


def main():
    kit = Path(sys.argv[1])
    manifest = json.loads((kit / "manifest.json").read_text())
    states = {t["path"]: t["state"] for t in manifest["tracks"]}
    tracks = [
        {"path": path, "chain": chain_of(preset_of(kit / states[path]), i + 1)}
        for i, path in enumerate(KICK)
    ]
    json.dump({"session": manifest["session"], "tracks": tracks}, sys.stdout, indent=1)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
