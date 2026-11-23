# Crescendum Album Benchmarking Protocol (`rhythm-bench`)

All rhythm & drum detection models in `FastTrackStudios/rhythm` are evaluated with the **exact same harness (`crates/rhythm-bench`)** so every row in [Tracker #15](https://github.com/FastTrackStudios/rhythm/issues/15) is strictly apples-to-apples.

## 1. Benchmark Corpus
9 reunited comped multi-mic live rock/metal drum sessions (48 kHz, 24-bit WAV, 140–190 BPM):
```
/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSIONS-2026-10-08/Crescendum/
  Chained expectations/Chained expectations.comped.RPP   (190 BPM)
  Intro/Intro.comped.RPP                                 (180 BPM)
  Kornesque /Kornesque .comped.RPP                       (180 BPM)
  The ballad/The ballad.comped.RPP                       (140 BPM)
  heavify/heavify.comped.RPP                             (170 BPM)
  it knows my name/it knows my name.comped.RPP           (160 BPM)
  set in stone/set in stone.comped.RPP                   (180 BPM)
  the plague/the plague.comped.RPP                       (165 BPM)
  unbreakable/unbreakable.comped.RPP                     (160 BPM)
```

### Track Layout & Active Comp Rule
- Every song has a live `Drums` folder containing:
  - `Kick`: `In`, `Out`, `Trig`
  - `Snare`: `Top`, `Bottom`, `Trig`
  - `Toms`: `T1`, `T2`, `T3`, `T4` + `Trig`: `T1 Trig`, `T2 Trig`, `T3 Trig`, `T4 Trig`
  - `Cymbals`: `OHs`, `Hi Hat`, `Ride`
  - `Rooms`
- All live drum tracks use REAPER Fixed Item Lanes with `LANESOLO 1 0 0 0 0 0 0 0` — **only lane 0 (`C1`/`C2`) is active**. `rhythm-bench` renders only the active comped items with `POSITION`, `LENGTH`, `SOFFS`, `VOLPAN`, and linear `FADEIN`/`FADEOUT` crossfades.

## 2. Ground Truth & Scoring Semantics
- Ground-truth onsets for `kick`, `snare`, and `t1`–`t4` are extracted deterministically from the 6 phase-locked hardware piezo trigger tracks via `rhythm_hits::trigger::spikes(..., 28.0, 0.030)`.
- Every trigger is checked with `rhythm_hits::calibrate::calibrate` + `rhythm_hits::trigger::health`. Any broken/chattering hardware trigger (e.g. a dead tom trigger) is automatically marked `dead` and excluded from scoring so bad hardware never corrupts F1.
- Scoring uses 1-to-1 greedy nearest-pair matching (`mir_eval.onset` semantics) at **±20 ms** (primary drum-editing tolerance) and **±50 ms** (standard MIR tolerance), grouped into canonical classes **`kick`**, **`snare`**, and **`toms`** (`t1`–`t4` merged) so 6-mic DSP detectors and 5/7-class neural models (`BD`, `SD`, `TT`, `HH`, `CY`) share identical ground truth.
- Acoustic propagation delay note: piezo triggers fire at stick impact (`0 ms`); close mics lag by ~0.5–1.5 ms; overheads lag by ~3–5 ms; rooms lag by ~10–20 ms. Both `median_abs_err_ms` and `median_signed_err_ms` are reported per class.

## 3. Running Benchmarks

### A. Rust detectors registered in `DetectorRegistry`
```bash
# Fair acoustic multi-mic benchmark (no triggers passed to detector):
cargo run --release -p rhythm-bench --   --detector fts.hits.kit-fusion   --mode mic-only   --out benchmarks/kit-fusion-mic-only.json

# ADTOF requires the optional model pack in models.json and ORT_DYLIB_PATH.
# Raw frame peaks and drum-bus attack snapping:
for variant in adtof-onnx-raw adtof-onnx; do
  cargo run --release -p rhythm-bench --features onnx -- \
    --detector "fts.hits.$variant" --mode bus-only --out "benchmarks/$variant.json"
done

# Overhead attack snapping and class-gated close-mic attacks:
for variant in adtof-onnx-ohs adtof-onnx-close; do
  cargo run --release -p rhythm-bench --features onnx -- \
    --detector "fts.hits.$variant" --mode mic-only --out "benchmarks/$variant.json"
done

# Fast single-song iteration while developing:
cargo run --release -p rhythm-bench -- --song "set in stone" --detector <id>
```

### B. Python / `uv` / PyTorch / ONNX prototypes (zero RPP parsing needed!)
1. **Export rendered active-comp WAV stems + `gt_onsets.json` once:**
   ```bash
   cargo run --release -p rhythm-bench --      --export-stems "$HOME/.cache/fts-rhythm/crescendum-stems"
   ```
   Each song folder contains `drum_bus.wav`, `kick_in.wav`, `snare_top.wav`, `t1.wav`..`t4.wav`, `ohs.wav`, `hihat.wav`, `ride.wav`, `rooms.wav` (48 kHz mono float WAV) and `gt_onsets.json`.
2. **Run your model (with `uv run`) and write predictions JSON:**
   ```json
   {
     "engine": "adtof-plus",
     "cpu_rtf": 0.14,
     "songs": {
       "set in stone": [
         { "piece": "kick", "at": 2.914 },
         { "piece": "snare", "at": 3.247 },
         { "piece": "toms", "at": 14.510 }
       ]
     }
   }
   ```
   Accepted piece aliases: `kick`/`bd`, `snare`/`sd`, `toms`/`tt`/`t1`..`t4`, `hihat`/`hh`, `cymbals`/`cy`/`crash`/`ride`.
3. **Score identically with `rhythm-bench`:**
   ```bash
   cargo run --release -p rhythm-bench --      --predictions /tmp/adtof_plus_preds.json      --out benchmarks/adtof-plus.json
   ```

### C. Beat and downbeat tracking

Download both `beat-this-small0` and `beat-this-logmel` as listed in
`models.json`. Score the drum-bus results against the REAPER project grid:

```bash
cargo run --release -p rhythm-bench --features onnx -- \
  --detector fts.hits.beat-this --beat-grid --out benchmarks/beat-this-small0.json
```

Beat and downbeat F1 are scored separately at ±50 ms and ±70 ms with
one-to-one matching. The reference includes tempo and time-signature changes
from REAPER step markers. It includes the whole rendered timeline, including
silent count-in and gaps. The report also gives per-song median inter-beat
BPM for identifying half-time and double-time tracking.

The `fts.hits.adtof-beat-this` detector emits both hits and beats in the same
response when all three models are cached. Build `rhythm-ext` with
`--features onnx` and pass the mono input as the `drum_bus` piece.
