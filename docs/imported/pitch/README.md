# pitch

Swappable pitch detection, note segmentation, and ARA-style incremental cached-span pitch editing as an out-of-process DAW extension (`fts.pitch`).

## Why this exists

- Keeps pitch analysis, note-blob editing, and future neural backends (FCPE / RMVPE / GAME / PC-NSF-HiFiGAN via ONNX Runtime, like [PitchNet](https://github.com/SessionLoops/PitchNet)) completely outside `fts-fx` and `daw`.
- Uses an ARA-like workflow over process IPC: analyze source audio into a canonical note-blob document, edit individual notes, and incrementally resynthesize edited spans to cached audio files that the DAW plays back lock-free.
- Every stage — **detector**, **segmenter**, and **synthesizer** — is swappable by ID so classical DSP and neural/third-party backends share one contract.

## Crates

- `crates/pitch-core`: Ported from `fts-fx/features/dsp/src/tune` (YIN f0, note segmentation, scale correction, DNA harmonic separation, `PitchDoc`/`NoteBlob`) plus swappable `PitchDetector`, `NoteSegmenter`, and `PitchSynthesizer` traits and registry.
- `crates/pitch-ext`: Process extension (`fts-pitch` binary + `extension.toml`) with stdio JSON-RPC server, incremental cached-span WAV renderer, and CLI.

## Prototype test

```bash
cargo test --workspace
```
