"""Summarize paired per-track differences relative to a baseline."""
import argparse
from collections import defaultdict
import json
from math import isfinite
from pathlib import Path
from statistics import median


def load_scores(paths, metric):
    scores = defaultdict(dict)
    tracks = defaultdict(set)
    seen = set()
    systems = {}
    protocol = None
    for path in paths:
        report = json.loads(path.read_text(encoding="utf-8"))
        if report["version"] != 1:
            raise ValueError(f"{path}: unsupported report version")
        if protocol is None:
            protocol = report["protocol"]
        if report["protocol"] != protocol:
            raise ValueError(f"{path}: scoring protocol differs from the first report")
        for system, metadata in report["manifest"]["systems"].items():
            if systems.setdefault(system, metadata) != metadata:
                raise ValueError(f"{path}: system metadata changed for {system}")
        for row in report["scores"]:
            track, system, stem = (row[key] for key in ("track", "system", "stem"))
            key = (track, system, stem)
            if key in seen:
                raise ValueError(f"{path}: duplicate score row {key}")
            seen.add(key)
            tracks[stem].add(track)
            value = row[metric]
            if value is not None and not isfinite(value):
                raise ValueError(f"{path}: non-finite score for {key}")
            scores[system, stem][track] = value
    return scores, tracks, systems, protocol


def summarize(paths, baseline, metric):
    scores, tracks, systems, protocol = load_scores(paths, metric)
    if baseline not in systems:
        raise ValueError(f"baseline system {baseline!r} is absent")
    results = []
    for system, stem in sorted(scores):
        if system == baseline:
            continue
        base = scores.get((baseline, stem), {})
        candidate = scores[system, stem]
        pairs = sorted(t for t in base.keys() & candidate.keys()
                       if base[t] is not None and candidate[t] is not None)
        deltas = [candidate[t] - base[t] for t in pairs]
        results.append({
            "system": system, "stem": stem,
            "tracks_in_reports": len(tracks[stem]), "paired_tracks": pairs,
            "median_delta_db": median(deltas) if deltas else None,
            "deltas_db": dict(zip(pairs, deltas)),
        })
    return {"baseline": baseline, "metric": metric, "protocol": protocol,
            "systems": systems, "comparisons": results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scores", nargs="+", type=Path)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--metric", default="global_sdr_db",
                        choices=("global_sdr_db", "bss_sdr_db", "bss_sir_db", "bss_isr_db", "bss_sar_db"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = summarize(args.scores, args.baseline, args.metric)
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(report, output, indent=2, allow_nan=False)
            output.write("\n")
    except (KeyError, ValueError, OSError) as error:
        parser.exit(1, f"Summary failed: {error}\n")
