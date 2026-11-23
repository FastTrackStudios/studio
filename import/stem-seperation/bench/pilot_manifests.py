"""Map the open-model pilot exports to common scoring groups."""
import argparse
import json
from pathlib import Path


def demucs(model, stem):
    return f"{model}/{model}/mix/{stem}.wav"


def labeled_exports(root, system, labels):
    paths = list((root / system).glob("*.wav"))

    def find(label):
        matches = [path for path in paths if f"_({label.lower()})_" in path.name.lower()]
        if len(matches) != 1:
            raise ValueError(f"expected one {system} {label} export, found {matches}")
        return str(matches[0].relative_to(root))

    return {stem: [find(label) for label in names] for stem, names in labels.items()}


DRUMSEP_LABELS = {"kick": ["Kick"], "snare": ["Snare"], "toms": ["Toms"],
                  "hihat": ["Hh"], "cymbals": ["Ride", "Crash"]}


def crescendum_manifest(root):
    fixture = json.loads((root / "reference/fixture.json").read_text())
    groups = fixture["groups"]
    stems = {stem: f"reference/{stem}.wav" for stem in groups}
    exports = {"larsnet": {stem: f"larsnet/{stem}/crescendum.wav" for stem in groups},
               "drumsep": labeled_exports(root, "drumsep", DRUMSEP_LABELS)}
    systems = {"larsnet": {"device": "cpu", "wiener": False,
                           "weights_archive_sha256": "dd785d77f66413c5acbd3a4cedac02951578c838496d059930c43f159647c907"},
               "drumsep": {"device": "cpu", "frontend": "audio-separator 0.48.0",
                           "batch_size": 1, "overlap": 2,
                           "checkpoint_sha256": "d2a4aa53eb584d21eead358a4e66d1882ad182911be018f052b5da73be9096d0"}}
    payload = {"version": 1, "systems": systems, "fixture": fixture,
               "tracks": [{"id": fixture["song"], "reference": stems, "estimates": exports}]}
    (root / "kit.json").write_text(json.dumps(payload, indent=2) + "\n")


def write_manifest(root, task, track, references, estimates, systems):
    metadata = json.loads((root / "references/fixtures.json").read_text())
    reference = {stem: [f"references/{track}/{part}.wav" for part in parts]
                 for stem, parts in references.items()}
    payload = {"version": 1, "systems": systems, "fixture": metadata[track],
               "tracks": [{"id": track, "reference": reference, "estimates": estimates}]}
    (root / f"{task}.json").write_text(json.dumps(payload, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path, help="Pilot directory containing references and exports")
    parser.add_argument("task", choices=["four", "six", "vocals", "crescendum"])
    parser.add_argument("--bs-roformer", action="store_true")
    parser.add_argument("--scnet", action="store_true")
    args = parser.parse_args()
    if args.task != "four" and (args.bs_roformer or args.scnet):
        parser.error("full-mix candidates require the four-stem task")
    root, task = args.output, args.task
    if task == "crescendum":
        crescendum_manifest(root)
        return
    if task == "vocals":
        models = ("UVR_MDXNET_KARA", "UVR_MDXNET_KARA_2")
        labels = {"lead": ["Vocals"], "backing": ["Instrumental"]}
        write_manifest(root, task, "patrick-vocals", {"lead": ["lead"], "backing": ["backing"]},
                       {model: labeled_exports(root, model, labels) for model in models},
                       {model: {"device": "cpu", "frontend": "audio-separator 0.48.0",
                                "checkpoint": f"{model}.onnx"} for model in models})
        return
    groups = {stem: [stem] for stem in ("vocals", "drums", "bass", "other")}
    if task == "six":
        groups.update(guitar=["guitar"], piano=["piano"])
    else:
        groups["other"] += ["guitar", "piano"]
    models = ("htdemucs", "htdemucs_6s") if task == "four" else ("htdemucs_6s",)
    estimates = {model: {stem: demucs(model, stem) for stem in groups} for model in models}
    if task == "four":
        estimates["htdemucs_6s"]["other"] = [demucs("htdemucs_6s", part) for part in groups["other"]]
    hashes = {
        "htdemucs": "8726e21a993978c7ba086d3872e7608d7d5bfca646ca4aca459ffda844faa8b4",
        "htdemucs_6s": "34c22ccb381c6f9fdbf324f04e1e2fe21aaaf293f5ded163a162697ff9a02ddd",
    }
    systems = {model: {"device": "cpu", "frontend": "demucs 4.0.1",
                       "checkpoint_sha256": hashes[model]} for model in models}
    if args.bs_roformer:
        estimates["bs-roformer"] = labeled_exports(root, "bs-roformer", {
            stem: [stem] for stem in groups
        })
        systems["bs-roformer"] = {
            "device": "cpu", "frontend": "audio-separator 0.48.0",
            "checkpoint": "model_bs_roformer_ep_17_sdr_9.6568.ckpt",
            "checkpoint_sha256": "3e9daecd70aaed5b5a0d1f861cc4d77eaa45afb3fc6301b1cf32c1be0f5868fb",
            "batch_size": 1, "overlap": 2,
        }
    if args.scnet:
        estimates["scnet"] = {stem: f"scnet/patrick-song/{stem}.wav" for stem in groups}
        systems["scnet"] = {
            "device": "cpu", "frontend": "starrytong/SCNet", "overlap": 0.5,
            "checkpoint_sha256": "f8aff3bc8dd816aad392800d9d69519add719a2fbd22550af6f8cb2138d25ff5",
            "source_archive_sha256": "8004d4219d3b75d6b7e9113c1d57a461204b9fda72bc5ab80c0f9c2383521ae6",
        }
    write_manifest(root, task, "patrick-song", groups, estimates, systems)


if __name__ == "__main__":
    main()
