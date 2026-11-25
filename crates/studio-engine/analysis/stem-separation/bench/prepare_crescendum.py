"""Build a matched comped Crescendum drum pilot from rhythm-bench WAV exports."""
import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf

from prepare_pilot import RATE, read_wav

GROUPS = {"kick": ["kick_in"], "snare": ["snare_top"],
          "toms": ["t1", "t2", "t3", "t4"], "hihat": ["hihat"],
          "cymbals": ["ride"]}
SECONDS = 12


def select_excerpt(pieces, duration):
    classes = ("kick", "snare", "toms")
    labels = {name: sorted(t for p in pieces if p["healthy"] and p["canonical_class"] == name
                           for t in p["onsets_sec"]) for name in classes}
    starts = range(0, int(duration) - SECONDS + 1, 2)
    start = max(starts, key=lambda s: min(sum(s <= t < s + SECONDS for t in times)
                                          for times in labels.values()))
    return start, {key: [t - start for t in times if start <= t < start + SECONDS]
                   for key, times in labels.items()}


def prepare(source, output):
    gt = json.loads((source / "gt_onsets.json").read_text())
    start, onsets = select_excerpt(gt["pieces"], gt["duration_secs"])
    if any(not hits for hits in onsets.values()):
        raise ValueError(f"no excerpt with all three healthy trigger classes: {source.name}")
    stems = {name: sum(read_wav(source / f"{mic}.wav", start, SECONDS) for mic in mics)
             for name, mics in GROUPS.items()}
    mix = sum(stems.values())
    gain = .9 / max(np.abs(mix).max(), *(np.abs(x).max() for x in stems.values()))
    output.mkdir(parents=True)
    for name, audio in {**stems, "mix": mix}.items():
        sf.write(output / f"{name}.wav", audio * gain, RATE, subtype="FLOAT")
    metadata = {"song": gt["song"], "start_seconds": start, "duration_seconds": SECONDS,
                "reference": "active-comp close microphones; cymbals reference is the ride mic",
                "groups": GROUPS, "gain": float(gain), "onsets_seconds": onsets,
                "trigger_health": [{k: p[k] for k in ("piece", "healthy", "health_note")}
                                   for p in gt["pieces"]]}
    (output / "fixture.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("song", type=Path, help="Song directory exported by rhythm-bench")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    prepare(args.song, args.output)
