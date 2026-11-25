"""Add unrelated accompaniment to known Crescendum drums at equal RMS."""
import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf

BACKING = ("bass", "guitar", "piano", "vocals", "other")


def prepare(kit, song, output):
    drums, rate = sf.read(kit / "mix.wav", always_2d=True)
    backing = np.zeros_like(drums)
    for stem in BACKING:
        samples, source_rate = sf.read(song / f"{stem}.wav", always_2d=True, frames=len(drums))
        if source_rate != rate or samples.shape != drums.shape:
            raise ValueError(f"{stem}: accompaniment must match kit sample rate, channels and duration")
        backing += samples
    backing_norm = np.linalg.norm(backing)
    drum_norm = np.linalg.norm(drums)
    if not backing_norm or not drum_norm:
        raise ValueError("both inputs must be audible")
    backing_gain = drum_norm / backing_norm
    mix = drums + backing * backing_gain
    gain = min(1.0, .99 / np.abs(mix).max())
    output.mkdir()
    sf.write(output / "mix.wav", mix * gain, rate, subtype="FLOAT")
    sf.write(output / "drums.wav", drums * gain, rate, subtype="FLOAT")
    metadata = {"backing_stems": BACKING, "backing_gain": float(backing_gain),
                "mix_gain": float(gain), "backing_to_drum_rms_db": 0}
    (output / "mixture.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("kit", "song", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    prepare(args.kit, args.song, args.output)
