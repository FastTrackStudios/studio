# dsp

Pure Rust audio DSP library for FastTrackStudio — **zero UI, zero GPU, zero plugin-shell dependencies**.

Extracted from `FastTrackStudios/fts-fx` so all tightly interwoven effect DSP (EQ, dynamic/spectral bands, compressors, gates, limiters, levelers, saturation, reverb, delay, modulation, multiband crossovers, mic modeling, metering, and stage stacks) can evolve, compile, and test in seconds without pulling in `dioxus`, `wgpu`, `blitz`, or `nice-plug`.

## Crates
- `crates/dsp-core` — Core building blocks: filters, buffers, oversampling, smoothing, modulation, EBU R128 loudness, musical time, golden-master harness.
- `crates/dsp` — Feature-gated effect engines (`eq`, `comp`, `gate`, `level`, `saturate`, `reverb`, `delay`, `modulation`, `multiband`, `mic`, `pitch`, `trigger`, `align`, `analyzer`, `stack`) + hardware profiles.

## Related Repos
- `FastTrackStudios/plugins` — Plugin UIs, `fts-audio-ui`, `plugin-core`, `preset-browser`, and CLAP/VST3/standalone targets (depends on `dsp`).
- `FastTrackStudios/rhythm` — Swappable drum/rhythm hit detection & hybrid editing extension.
- `FastTrackStudios/pitch` — Swappable F0 pitch detection, note segmentation & ARA pitch correction extension.
- `FastTrackStudios/stretch` — Swappable real-time time-stretch & pitch-shift C-ABI plugins.
- `FastTrackStudios/stem-seperation` — Embeddable multi-model stem separation library & CLI.

## Building & Testing
```bash
cargo check --workspace --all-features
cargo test --workspace --all-features
```
