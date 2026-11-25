# Agent Guide — `FastTrackStudios/pitch`

This repository implements swappable pitch detection, note segmentation, and ARA-style pitch correction (`fts.pitch`) using the same public contracts available to third-party developers.

## Hard Rules & Environment
- **Scope:** Only modify files inside this repo (`/run/media/Development/fasttrackstudio/pitch`). **NEVER** touch sibling repos (`studio`, `daw`, `fts-fx`, `signal`, `session`, etc.) — other agents may be editing them concurrently.
- **Cargo linker quirk:** Always run `cargo` with `Cwd` inside this repo (or `--manifest-path` from a neutral directory) — **never** from `/run/media/Development/fasttrackstudio/studio`, which sets `-fuse-ld=mold`.
- **Hardware & Python:** An **NVIDIA GeForce RTX 4080 (16 GB)** is available (`nvidia-smi`). Global `python3` has no ML packages installed — use **`uv`** (`/run/current-system/sw/bin/uv`, e.g. `uv run --with onnxruntime --with numpy --with soundfile ...` or a repo-local `.venv` listed in `.gitignore`).
- **No large binaries in git:** NEVER commit `.onnx`, `.pt`, `.ckpt`, `.safetensors`, or `.wav` files. Store downloaded weights under `${FTS_PITCH_MODEL_DIR:-$HOME/.cache/fts-pitch/models}`.
- **Licensing hygiene:** Keep repo code cleanly open-source (`GPL-3.0-or-later`). Tag any `CC BY-NC` / academic weights as optional downloadable model packs in a `models.json` manifest rather than bundling them into git.

## Code Entry Points
- `crates/pitch-core/src/backends.rs` — Core traits & registry:
  - `PitchDetector` (`fts.pitch.yin` baseline)
  - `NoteSegmenter` (`fts.segment.contour` baseline)
  - `PitchSynthesizer` (`fts.synth.td-ola` baseline)
  - `PitchAnalysisDoc`, `FrameRecord`, `EditableNote`
- `crates/pitch-ext/src/lib.rs` — Process extension dispatch (`PITCH_ANALYZE`, `PITCH_CORRECT_SCALE`, `PITCH_RENDER_SPAN`)
- `crates/pitch-ext/tests/workflow.rs` — End-to-end WAV roundtrip test (`cargo test --workspace`)

## Local Benchmark Datasets
- **MIR Vocal & Pitch Datasets:** `/run/media/AudioHaven/mir-datasets/data/`
  - `vocadito/` — Monophonic solo singing with F0 + note annotations
  - `dagstuhl_choirset/` — Multi-track choral singing with F0 ground truth
  - `MDB-stem-synth/` — Exact F0 ground truth across vocals & instruments
  - `GuitarSet/` — Polyphonic guitar pitch & note annotations (great for Basic Pitch)
- **Live Album Vocals (Crescendum):** `/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSIONS-2026-10-08/Crescendum/<song>/<song>.comped.RPP` (active comp lane 0 via `LANESOLO 1`, tracks under `VOX BUS` / `LEAD VOX BUS`).

## Definition of Done for Benchmark Issues
1. Work in a feature branch and open a PR (or commit cleanly per repo workflow).
2. Wire the model behind an optional `onnx` Cargo feature (`ort = "2.0.0-rc.15"`) implementing the corresponding trait in `crates/pitch-core/src/backends.rs` (or a clean prototype harness if evaluating before Rust port).
3. Run `cargo test --workspace` and verify zero regressions.
4. Save benchmark JSON under `benchmarks/<model>.json`.
5. Post a structured results comment on the GitHub issue (RPA @ ±50¢ & ±10¢, voicing F1, note onset/offset F1, CPU & GPU RTF, RAM, model size, license tier, auditioned verdict) and close the issue once complete.
