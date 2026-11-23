"""Build a paired oracle/cascade scoring manifest for one kit fixture."""
import argparse
import json
from pathlib import Path
import os

import soundfile as sf

from pilot_manifests import DRUMSEP_LABELS, labeled_exports


def paired_manifest(oracle, cascade, mixture_gain):
    payload = json.loads(oracle.read_text())
    track, = payload["tracks"]
    for stem, path in track["reference"].items():
        track["reference"][stem] = export_scaled(oracle.parent / path, cascade, stem, mixture_gain)
    for system, stems in track["estimates"].items():
        for stem, files in stems.items():
            files = [files] if isinstance(files, str) else files
            stems[stem] = [export_scaled(oracle.parent / file, cascade, f"{system}-{stem}-{i}", mixture_gain)
                           for i, file in enumerate(files)]
    payload["mixture"] = {"description": "cross-song interference stress test",
                          "backing_to_drum_rms_db": 0, "mix_gain": mixture_gain}
    payload["onset_mix"] = os.path.relpath(oracle.parent / "reference/mix.wav", cascade)
    payload["onset_baselines"] = {
        "contaminated_mix": "mix.wav",
        "htdemucs_drums": "extracted/htdemucs/mix/drums.wav",
    }
    track["estimates"].update(
        htdemucs_larsnet={stem: f"larsnet/{stem}/crescendum.wav" for stem in track["reference"]},
        htdemucs_drumsep=labeled_exports(cascade, "drumsep", DRUMSEP_LABELS),
    )
    for system in ("larsnet", "drumsep"):
        payload["systems"][f"htdemucs_{system}"] = {
            **payload["systems"][system], "first_stage": "htdemucs 4.0.1",
        }
    return payload


def export_scaled(path, root, name, gain):
    if gain == 1:
        return os.path.relpath(path, root)
    audio, rate = sf.read(path, always_2d=True)
    target = root / "scaled" / f"{name}.wav"
    target.parent.mkdir(exist_ok=True)
    sf.write(target, audio * gain, rate, subtype="FLOAT")
    return str(target.relative_to(root))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("oracle_manifest", type=Path)
    parser.add_argument("cascade", type=Path)
    parser.add_argument("--gain", required=True, type=float)
    args = parser.parse_args()
    if not 0 < args.gain <= 1:
        parser.error("--gain must be greater than zero and at most one")
    result = paired_manifest(args.oracle_manifest, args.cascade, args.gain)
    (args.cascade / "paired.json").write_text(json.dumps(result, indent=2) + "\n")
