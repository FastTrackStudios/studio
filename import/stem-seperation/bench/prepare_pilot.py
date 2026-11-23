"""Build a matched song fixture from local Cambridge multitracks."""
import argparse
import json
from pathlib import Path

import numpy as np
from scipy.signal import resample_poly
import soundfile as sf
from math import gcd

RATE = 44100
SECONDS = 20
FIXTURES = {
    "patrick-vocals": ("PatrickTalbot_AReasonToLeave", {
        "lead": [32, 33], "backing": list(range(34, 39)),
    }),
    "patrick-song": ("PatrickTalbot_AReasonToLeave", {
        "drums": [1, 4, 7, 8, 9, 10, 11, 17, 19], "bass": [20],
        "guitar": list(range(21, 29)), "piano": [30], "other": [29, 31],
        "vocals": list(range(32, 39)),
    }),
}


def read_wav(path, start, duration):
    with sf.SoundFile(path) as source:
        rate = source.samplerate
        # Read context across the cut so the resampling filter sees the same
        # neighbors as resampling the full track before cropping.
        context = min(start, 1) if rate != RATE else 0
        source.seek(min(len(source), (start - context) * rate))
        count = (duration + context + int(rate != RATE)) * rate
        data = source.read(count, always_2d=True, dtype="float32")
    data = np.pad(data, ((0, count - len(data)), (0, 0)))
    if rate != RATE:
        common = gcd(rate, RATE)
        data = resample_poly(data, RATE // common, rate // common, axis=0)
    data = data[context * RATE:(context + duration) * RATE]
    if data.shape[1] == 1:
        data = np.repeat(data, 2, axis=1)
    if data.shape[1] != 2:
        raise ValueError(f"expected mono or stereo: {path}")
    return data


def prepare(root, output, groups, start):
    files = {int(p.name[:2]): p for p in root.rglob("*.wav")
             if p.name[:2].isdigit()}
    stems = {name: sum(read_wav(files[i], start, SECONDS) for i in ids)
             for name, ids in groups.items()}
    silent = [name for name, audio in stems.items() if not np.any(audio)]
    if silent:
        raise ValueError(f"silent reference groups in selected excerpt: {silent}")
    mix = sum(stems.values())
    gain = .9 / max(np.abs(mix).max(), *(np.abs(audio).max() for audio in stems.values()))
    output.mkdir(parents=True, exist_ok=False)
    for name, audio in {**stems, "mix": mix}.items():
        sf.write(output / f"{name}.wav", audio * gain, RATE, subtype="FLOAT")
    return {"source": root.name, "start_seconds": start,
            "duration_seconds": SECONDS, "gain": float(gain),
            "sources": {name: [files[i].name for i in ids] for name, ids in groups.items()}}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("cambridge", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--fixture", choices=FIXTURES)
    parser.add_argument("--start", type=int, required=True)
    args = parser.parse_args()
    if args.start < 0:
        parser.error("--start must be nonnegative")
    metadata = {name: prepare(args.cambridge / source, args.output / name, groups, args.start)
                for name, (source, groups) in FIXTURES.items()
                if args.fixture is None or name == args.fixture}
    (args.output / "fixtures.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    main()
