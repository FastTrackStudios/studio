"""Validate an unclipped session pair and measure first-stage drum separation."""
import argparse
import json
from pathlib import Path

import numpy as np
from scipy.signal import correlate
import soundfile as sf


def read(folder, path, shape, rate):
    audio, source_rate = sf.read(folder / path, always_2d=True)
    if source_rate != rate or audio.shape != shape or not np.isfinite(audio).all():
        raise ValueError(f"{path}: unexpected rate, layout or non-finite samples")
    return audio


def alignment(signal, reference, rate):
    x, y = signal.mean(axis=1), reference.mean(axis=1)
    scale = np.linalg.norm(x) * np.linalg.norm(y)
    if not scale:
        raise ValueError("cannot align a silent signal")
    middle = len(y) - 1
    window = min(rate // 10, middle)
    corr = correlate(x, y, method="fft")[middle - window:middle + window + 1] / scale
    peak = np.argmax(np.abs(corr))
    return {"lag_samples": int(peak - window), "peak_correlation": float(corr[peak])}


def check(folder, kit):
    reference, rate = sf.read(kit / "mix.wav", always_2d=True)
    fit = {
        name: read(folder, name + ".wav", reference.shape, rate) for name in ("mix", "drums")
    }
    peaks = {name: float(np.max(np.abs(audio))) for name, audio in fit.items()}
    if any(not 0 < peak < 1 for peak in peaks.values()):
        raise ValueError("expected audible, unclipped session renders")
    offsets = {name: alignment(audio, reference, rate) for name, audio in fit.items()}
    if any(abs(row["lag_samples"]) > 2 or row["peak_correlation"] < .3 for row in offsets.values()):
        raise ValueError(f"session render does not align with the matched kit: {offsets}")
    recovered = read(folder, "extracted/htdemucs/mix/drums.wav", reference.shape, rate)
    ref = fit["drums"]
    energy = np.sum(ref ** 2)
    def global_sdr(audio):
        error = np.sum((ref - audio) ** 2)
        return float(10 * np.log10(energy / error)) if error else None

    return {
        "alignment_to_close_mic_sum": offsets,
        "peaks": peaks,
        "global_sdr_db_against_solo": {
            name: global_sdr(audio)
            for name, audio in (("full_mix", fit["mix"]), ("htdemucs_drums", recovered))
        },
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("session", "kit", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    stats = check(args.session, args.kit)
    with args.output.open("x", encoding="utf-8") as f:
        json.dump(stats, f, indent=2, allow_nan=False)
        f.write("\n")
