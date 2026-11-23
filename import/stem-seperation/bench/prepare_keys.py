"""Build electric-keys targets and independent query examples."""
import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf

from prepare_pilot import RATE, SECONDS, read_wav

FIXTURES = {
    "patrick": ("PatrickTalbot_AReasonToLeave", 75, [29, 30],
                [1, 4, 7, 8, 9, 10, 11, 17, 19, 20, *range(21, 29), 31, *range(32, 39)]),
    "james": ("JamesMay_AllSoulsMoon", 45, [10, 11],
              [1, 2, 4, 5, 6, 8, 9, *range(12, 17)]),
    "dino": ("DinoOnTheLoose_QueensLight", 45, [19, 20],
             [1, 3, 4, 5, 6, 7, *range(8, 19)]),
}
QUERIES = {
    "patrick": ("james", 11),
    "james": ("patrick", 29),
    "dino": ("james", 11),
}


def indexed_files(root):
    return {int(p.name[:2]): p for p in root.rglob("*.wav") if p.name[:2].isdigit()}


def prepare(cambridge, root):
    for name, (song, start, keys_ids, remainder_ids) in FIXTURES.items():
        files = indexed_files(cambridge / song)
        keys = sum(read_wav(files[i], start, SECONDS) for i in keys_ids)
        remainder = sum(read_wav(files[i], start, SECONDS) for i in remainder_ids)
        mix = keys + remainder
        query_song, query_id = QUERIES[name]
        query_dir, query_start, _, _ = FIXTURES[query_song]
        query_file = indexed_files(cambridge / query_dir)[query_id]
        query = read_wav(query_file, query_start, 10)
        for label, audio in {"keys": keys, "remainder": remainder, "query": query}.items():
            if np.max(np.abs(audio)) < 1e-5:
                raise ValueError(f"{name}: {label} is silent")
        gain = .9 / max(np.abs(audio).max() for audio in (mix, keys, remainder))
        output = root / name
        output.mkdir(parents=True)
        for label, audio in {"mix": mix, "keys": keys, "remainder": remainder}.items():
            sf.write(output / f"{label}.wav", audio * gain, RATE, subtype="FLOAT")
        sf.write(output / "query.wav", query, RATE, subtype="FLOAT")
        metadata = {
            "track": song, "start": start, "duration": SECONDS, "gain": float(gain),
            "keys": [files[i].name for i in keys_ids],
            "remainder": [files[i].name for i in remainder_ids],
            "query": {"track": query_dir, "channel": query_file.name, "start": query_start},
        }
        (output / "fixture.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("cambridge", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    prepare(args.cambridge, args.output)
