"""Score aligned WAV exports against known stems."""
import argparse
import csv
from importlib.metadata import version
import json
import math
from pathlib import Path
import sys

try:
    import museval
    import numpy as np
    import soundfile as sf
except (ImportError, RuntimeError) as error:
    sys.exit(f"Benchmark dependency unavailable: {error}. See README for setup.")

METRICS = ("sdr", "isr", "sir", "sar")


def read_stems(root, stems, expected=None):
    if not isinstance(stems, dict) or not stems:
        raise ValueError("expected a non-empty stem mapping")
    audio = []
    for stem, files in stems.items():
        files = [files] if isinstance(files, str) else files
        if not isinstance(files, list) or not files:
            raise ValueError(f"{stem}: expected a WAV path or a non-empty array of paths")
        combined = None
        for file in files:
            path = root / file
            with sf.SoundFile(path) as source:
                if source.format not in ("WAV", "WAVEX", "RF64"):
                    raise ValueError(f"{path}: expected WAV audio")
                sample_rate = source.samplerate
                samples = source.read(always_2d=True)
            layout = (sample_rate, *samples.shape)
            expected = expected or layout
            if layout != expected:
                raise ValueError(f"{path}: expected rate/frames/channels {expected}, found {layout}")
            if not samples.size or not np.isfinite(samples).all():
                raise ValueError(f"{path}: empty or non-finite audio")
            if combined is None:
                combined = samples
            else:
                combined += samples
        audio.append(combined)
    return np.stack(audio), expected


def median_finite(values):
    finite = values[np.isfinite(values)]
    return float(np.median(finite)) if finite.size else None


def global_sdr(reference, estimate):
    energy = float(np.sum(reference ** 2))
    error = float(np.sum((reference - estimate) ** 2))
    if energy == 0 or error == 0:
        return None
    return 10 * math.log10(energy / error)


def score_export(track, system, stems, reference, estimate, rate, global_only):
    active = np.any(reference != 0, axis=(1, 2)) & np.any(estimate != 0, axis=(1, 2))
    # BSS Eval uses all stems to measure interference. A wholly silent stem
    # invalidates that joint evaluation, but global SDR remains available.
    scores = None
    if not global_only and active.all():
        scores = museval.evaluate(reference, estimate, win=rate, hop=rate,
                                  mode="v4", padding=False)
    for index, stem in enumerate(stems):
        ref, est = reference[index], estimate[index]
        status = ("silent_reference" if not np.any(ref) else
                  "silent_estimate" if not np.any(est) else
                  "perfect_match" if np.array_equal(ref, est) else "ok")
        row = dict(track=track, system=system, stem=stem, status=status,
                   global_sdr_db=global_sdr(ref, est))
        for name, frames in zip(METRICS, scores if scores is not None else [None] * 4):
            row[f"bss_{name}_db"] = None if frames is None else median_finite(frames[index])
        row["valid_bss_sdr_frames"] = (
            0 if scores is None else int(np.isfinite(scores[0][index]).sum()))
        yield row


def score_manifest(manifest, root, global_only):
    systems, tracks = manifest["systems"], manifest["tracks"]
    if manifest.get("version") != 1 or not systems or not tracks:
        raise ValueError("expected manifest version 1 with systems and tracks")

    seen = set()
    for track in tracks:
        track_id = track["id"]
        try:
            if track_id in seen:
                raise ValueError("duplicate track id")
            seen.add(track_id)
            stems = track["reference"]
            exports = track["estimates"]
            if len(stems) < 2 or exports.keys() != systems.keys():
                raise ValueError("expected at least two reference stems and exports for every system")
            reference, layout = read_stems(root, stems)
            for system, files in exports.items():
                if files.keys() != stems.keys():
                    raise ValueError(f"{system}: export stem names must match the reference")
                try:
                    estimate, _ = read_stems(root, {stem: files[stem] for stem in stems}, layout)
                    yield from score_export(track_id, system, stems, reference, estimate, layout[0], global_only)
                except (ValueError, RuntimeError, OSError, np.linalg.LinAlgError) as error:
                    raise ValueError(f"{system}: {error}") from error
        except (ValueError, RuntimeError, OSError) as error:
            raise ValueError(f"{track_id}: {error}") from error


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--global-only", action="store_true")
    args = parser.parse_args()
    if args.output.exists() or args.output.is_symlink():
        raise FileExistsError(f"output directory already exists: {args.output}")
    with args.manifest.open(encoding="utf-8") as source:
        manifest = json.load(source)
    scores = list(score_manifest(manifest, args.manifest.parent, args.global_only))
    report = {
        "version": 1,
        "protocol": {"bss_eval": None if args.global_only else "v4", "museval": version("museval"),
                     "window_seconds": 1, "hop_seconds": 1, "aggregation": "finite frame median"},
        "manifest": manifest,
        "scores": scores,
    }
    args.output.mkdir()
    with (args.output / "scores.json").open("w", encoding="utf-8") as output:
        json.dump(report, output, indent=2, allow_nan=False)
        output.write("\n")
    with (args.output / "scores.csv").open("w", encoding="utf-8", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=list(scores[0]))
        writer.writeheader()
        writer.writerows(scores)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError) as error:
        sys.exit(f"Benchmark failed: {error}")
