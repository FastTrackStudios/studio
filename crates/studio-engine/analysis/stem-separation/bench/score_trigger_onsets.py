"""Compare identical audio-only onset detectors against Crescendum trigger labels."""
import argparse
import json
from pathlib import Path

import librosa
import numpy as np
import soundfile as sf

HOP = 128
TOLERANCES = (20, 50)


def detect(root, files):
    files = [files] if isinstance(files, str) else files
    tracks = [sf.read(root / file, always_2d=True) for file in files]
    sample_rate = tracks[0][1]
    if any(rate != sample_rate or data.shape != tracks[0][0].shape
           for data, rate in tracks):
        raise ValueError("onset inputs must share rate, frames and channels")
    mono = sum(audio.mean(axis=1) for audio, _ in tracks)
    return librosa.onset.onset_detect(y=mono, sr=sample_rate, hop_length=HOP, units="time")


def compare(reference, detections, tolerance_ms):
    candidates = sorted((abs(actual - expected), i, j)
                        for i, expected in enumerate(reference)
                        for j, actual in enumerate(detections)
                        if abs(actual - expected) <= tolerance_ms / 1000)
    refs, preds, errors = set(), set(), []
    for _, i, j in candidates:
        if i not in refs and j not in preds:
            refs.add(i)
            preds.add(j)
            errors.append(1000 * (detections[j] - reference[i]))
    return {"hits": len(reference), "detections": len(detections), "matched": len(refs),
            "f1": 2 * len(refs) / (len(reference) + len(detections)),
            "median_signed_error_ms": float(np.median(errors)) if errors else None}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    track, = manifest["tracks"]
    truth = manifest["fixture"]["onsets_seconds"]
    coverage = {"kick": ("kick",), "snare": ("snare",), "toms": ("t1", "t2", "t3", "t4")}
    healthy = {piece["piece"] for piece in manifest["fixture"]["trigger_health"] if piece["healthy"]}
    baseline_paths = {"unseparated_mix": manifest.get("onset_mix", "reference/mix.wav"),
                      **manifest.get("onset_baselines", {})}
    baselines = {name: detect(args.manifest.parent, path) for name, path in baseline_paths.items()}
    results = []
    for piece, reference in truth.items():
        if not reference or not all(channel in healthy for channel in coverage[piece]):
            continue
        stems = {"close_mic": track["reference"][piece],
                 **{name: stems[piece] for name, stems in track["estimates"].items()}}
        sets = {**baselines, **{name: detect(args.manifest.parent, paths)
                                          for name, paths in stems.items()}}
        for system, detections in sets.items():
            for tolerance in TOLERANCES:
                results.append({"stem": piece, "system": system, "tolerance_ms": tolerance,
                                **compare(reference, detections, tolerance)})
    with args.output.open("x") as output:
        json.dump({"librosa": librosa.__version__, "hop_samples": HOP, "scores": results}, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
