# Agent Guide — `FastTrackStudios/rhythm`

This repository implements swappable drum/rhythm hit detection (`rhythm-hits`), hybrid slip+stretch editing (`rhythm-editing`), the `fts.rhythm` process extension (`rhythm-ext`), and the shared Crescendum album benchmark harness (`rhythm-bench`).

## Hard Rules & Environment
- **Scope:** Only modify files inside `/run/media/Development/fasttrackstudio/rhythm`. **NEVER** touch sibling repos (`studio`, `daw`, `fts-fx`, `signal`, `session`, etc.) — other agents may be editing them concurrently.
- **Cargo linker quirk:** Always run `cargo` with `Cwd` inside this repo (or via `--manifest-path` from a neutral directory) — **never** with `Cwd` inside `/run/media/Development/fasttrackstudio/studio`, which forces `-fuse-ld=mold`.
- **Hardware & Python:** An **NVIDIA GeForce RTX 4080 (16 GB)** is available (`nvidia-smi`). Global `python3` has no ML deps — use **`uv`** (`/run/current-system/sw/bin/uv`, e.g., `uv run --with onnxruntime-gpu --with numpy --with soundfile ...` or a repo-local `.venv` listed in `.gitignore`).
- **Rust ONNX (`--features onnx`):** `rhythm-hits` enables `ort = "2.0.0-rc.15"` with `load-dynamic` (no `openssl-sys` build dependency on NixOS). At runtime, point `ORT_DYLIB_PATH` to `libonnxruntime.so` (e.g., from a `uv` `onnxruntime` wheel or Nix store) and use `rhythm_hits::onnx::load_session(path)`.
- **No large binaries in git:** NEVER commit `.onnx`, `.pt`, `.ckpt`, `.safetensors`, or `.wav` files. Cache downloaded models under `${FTS_RHYTHM_MODEL_DIR:-$HOME/.cache/fts-rhythm/models}` and exported stems under `$HOME/.cache/fts-rhythm/crescendum-stems`.
- **Licensing hygiene:** Repo code is `GPL-3.0-or-later`. Keep built-in defaults zero-dep DSP or OSI-permissive (MIT/Apache-2.0). Any `CC BY-NC` / academic weights MUST be registered in `models.json` with `"tier": "non_commercial"` as an optional user-fetched model pack.

## Code Entry Points
- `crates/rhythm-hits/src/registry.rs` — Swappable `HitDetector` trait, `DetectorRegistry`, `DetectorInput` (includes `tempo_bpm`, `time_sig`), `DetectorOutput` (includes `hits`, `beats`, `sections`).
- `crates/rhythm-hits/src/attack.rs` — `attack_start()` and `attack_peak()` for snapping neural 10 ms frame peaks to sample-accurate waveform attacks.
- `crates/rhythm-hits/src/models.rs` & `models.json` — License-aware model pack manifest and cache resolver.
- `crates/rhythm-hits/src/onnx.rs` — Shared `ort` session loader (`--features onnx`).
- `crates/rhythm-editing/src/hybrid.rs` — Individual `HitMove` (`Auto` / `SlipOnly` / `Stretch`) and transient-protected `TailStretch` planner.
- `crates/rhythm-ext/src/lib.rs` — `fts-rhythm` CLI & stdio JSON-RPC server (`RHYTHM_STATE`, `RHYTHM_DETECT`, `RHYTHM_PLACE_HITS`, `RHYTHM_QUANTIZE_PLAN`).
- `crates/rhythm-bench/` — Shared Crescendum album benchmark harness (see [`BENCHMARKING.md`](BENCHMARKING.md)).

## Definition of Done for Every Model / Benchmark Issue
1. Work in a feature branch (e.g., `eval/<model-name>`) and open a PR (or commit cleanly).
2. Wire the detector into `DetectorRegistry` (behind `--features onnx` if neural) **or**, for Python-only evaluation models, save the reproducible evaluation script under `scripts/eval_<model>.py` (runnable via `uv run`).
3. Run the **shared benchmark harness** (`rhythm-bench`) across all 9 Crescendum songs — see [`BENCHMARKING.md`](BENCHMARKING.md) — so every model is scored on the **exact same active-comp audio, hardware trigger ground truth, and matching algorithm**.
4. Save canonical benchmark JSON to `benchmarks/<model-slug>.json` (small text JSON — commit this!) and ensure `cargo test --workspace` passes.
5. Post a structured benchmark summary comment on the GitHub issue, update the comparison table on **#15**, and close the issue once all checklist items are verified.
