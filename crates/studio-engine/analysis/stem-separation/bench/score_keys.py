"""Prepare one keys-versus-remainder manifest for aligned model estimates."""
import argparse
import json
from pathlib import Path

import soundfile as sf

from pilot_manifests import labeled_exports
from prepare_keys import FIXTURES


def complement(folder, model, keys):
    mix, rate = sf.read(folder / "mix.wav", always_2d=True)
    estimate, estimate_rate = sf.read(folder / keys, always_2d=True)
    if estimate_rate != rate or estimate.shape != mix.shape:
        raise ValueError(f"{folder}/{keys}: expected the same rate, frames and channels as the mix")
    target = f"{model}-remainder.wav"
    sf.write(folder / target, mix - estimate, rate, subtype="FLOAT")
    return target


def prepare(root, specialist, banquet):
    systems = {
        "htdemucs-piano": {
            "device": "cpu", "model": "htdemucs_6s", "output": "piano", "remainder": "mixture subtraction",
        },
    }
    if specialist:
        systems["digital-piano"] = {
            "device": "cpu", "model": "MVSep Mega 53 digital-piano head, community extraction",
            "checkpoint_sha256": "481c741ff36208c9b11860297ff5c9a502873662724d35a437bf64aaade890bd",
            "weight_terms": "unknown",
            "config_sha256": "84cce33eda082320c57be9ee5f8b14808a5b381f00d28b00a174dcea421ed9b4",
            "remainder": "mixture subtraction",
        }
    if banquet:
        systems["banquet"] = {
            "device": "cpu", "checkpoint": "ev-pre-aug.ckpt",
            "checkpoint_sha256": "657295888781e62ef50593002720d2edb3858b9e5bbfabf0c54f715a0da4b9e2",
            "weight_terms": "CC-BY-NC-SA-4.0",
            "chunk_seconds": 6, "hop_seconds": 3, "query_seconds": 10,
            "remainder": "mixture subtraction",
        }

    tracks = []
    for song, (title, _, _, _) in FIXTURES.items():
        folder = root / song
        keys_paths = {"htdemucs-piano": "htdemucs/htdemucs_6s/mix/piano.wav"}
        if specialist:
            keys_paths["digital-piano"], = labeled_exports(folder, "digital-piano", {
                "keys": ["digital-piano"],
            })["keys"]
        if banquet:
            keys_paths["banquet"] = "banquet/keys.wav"
        estimates = {}
        for model, keys in keys_paths.items():
            remainder = complement(folder, model, keys)
            estimates[model] = {"keys": f"{song}/{keys}", "remainder": f"{song}/{remainder}"}
        tracks.append({
            "id": title, "fixture": json.loads((folder / "fixture.json").read_text()),
            "reference": {stem: f"{song}/{stem}.wav" for stem in ("keys", "remainder")},
            "estimates": estimates,
        })
    name = "keys-" + "-".join(systems) + ".json"
    (root / name).write_text(json.dumps({"version": 1, "systems": systems, "tracks": tracks}, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--specialist", action="store_true")
    parser.add_argument("--banquet", action="store_true")
    args = parser.parse_args()
    prepare(args.output, args.specialist, args.banquet)
