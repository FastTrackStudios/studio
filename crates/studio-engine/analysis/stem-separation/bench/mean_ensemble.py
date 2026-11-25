"""Evaluate a fixed, untrained mean of the pilot's four-stem estimates."""
import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf

from score import read_stems


def prepare(manifest, systems):
    root = manifest.parent
    payload = json.loads(manifest.read_text())
    track, = payload["tracks"]
    stems = track["reference"]
    reference, layout = read_stems(root, stems)
    mix = reference.sum(axis=0)
    estimates = {system: read_stems(root, {stem: paths[stem] for stem in stems}, layout)[0]
                 for system, paths in track["estimates"].items()}
    if len(systems) < 2 or len(set(systems)) != len(systems):
        raise ValueError("select at least two distinct systems")
    mean = np.mean([estimates[system] for system in systems], axis=0)
    output = root / "mean-ensemble"
    output.mkdir()
    for stem, audio in zip(stems, mean):
        sf.write(output / f"{stem}.wav", audio, layout[0], subtype="FLOAT")
    track["estimates"]["mean-ensemble"] = {stem: f"mean-ensemble/{stem}.wav" for stem in stems}
    payload["systems"]["mean-ensemble"] = {"members": systems, "weights": [1 / len(systems)] * len(systems)}
    mix_energy = np.sum(mix ** 2)
    residual = {system: float(np.sum((mix - audio.sum(axis=0)) ** 2) / mix_energy)
                for system, audio in {**estimates, "mean-ensemble": mean}.items()}
    (root / "ensemble.json").write_text(json.dumps(payload, indent=2) + "\n")
    (output / "reconstruction.json").write_text(json.dumps({
        "residual_energy_fraction": residual,
    }, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("systems", nargs="+")
    args = parser.parse_args()
    prepare(args.manifest, args.systems)
