# Agent Guide — `FastTrackStudios/stretch`

This repository defines the zero-dependency C ABI (`daw_stretch_abi.h`) and swappable `cdylib` reference plugins for real-time time-stretching and pitch-shifting used by the DAW engine and `rhythm-editing`'s hybrid drum tail-stretch renderer.

## Hard Rules & Environment
- **Scope:** Only modify files inside this repo (`/run/media/Development/fasttrackstudio/stretch`). **NEVER** touch sibling repos (`studio`, `daw`, `fts-fx`, `signal`, `session`, etc.) — other agents may be editing them concurrently.
- **Cargo linker quirk:** Always run `cargo` with `Cwd` inside this repo (or `--manifest-path` from a neutral directory) — **never** from `/run/media/Development/fasttrackstudio/studio`, which sets `-fuse-ld=mold`.
- **No large binaries in git:** Never commit `.wav` test renders or `.so` build artifacts.

## Code Entry Points
- `include/daw_stretch_abi.h` — Canonical C ABI (`DAW_STRETCH_ABI_VERSION = 1`). Every plugin exports:
  ```c
  const DawStretchVTable* daw_stretch_vtable(void);
  ```
- `crates/stretch-abi/src/lib.rs` — Rust `#[repr(C)]` bindings + `host` dynamic `.so` loader (`libloading`).
- `crates/stretch-pv/src/lib.rs` — Phase-vocoder `cdylib` (`fts.stretch.pv`).
- `crates/stretch-wsola/src/lib.rs` — WSOLA `cdylib` (`fts.stretch.wsola`).
- `crates/stretch-host-tests/tests/swap_engines.rs` — Dynamic `dlopen` & hot-swap integration tests (`cargo test --workspace`).

## Local Drum Audio for Auditioning & Phase-Coherence Benchmarks
- **Crescendum Reunited Comped Drum Sessions (48 kHz 24-bit multi-mic):**
  `/run/media/AudioHaven/Project/Crescendum-Rockstars-SESSIONS-2026-10-08/Crescendum/<song>/<song>.comped.RPP`
- Or export ready-to-use active-comp WAV stems via `rhythm-bench` in `/run/media/Development/fasttrackstudio/rhythm`:
  ```bash
  cargo run --release -p rhythm-bench -- --export-stems ~/.cache/fts-rhythm/crescendum-stems --song "set in stone"
  ```
  Focus tests on `ohs.wav` (cymbal shimmer / comb-filtering), `rooms.wav` (room tail decay), `kick_in.wav`, and `snare_top.wav` at realistic drum-quantize ratios (`0.85x`–`1.15x`).

## Definition of Done for Benchmark / Engine Issues
1. Work in a feature branch and open a PR (or commit cleanly).
2. All `cdylib` plugins must pass `crates/stretch-host-tests/tests/swap_engines.rs` (`cargo test --workspace`).
3. Report block latency (µs per 512-frame block at 48 kHz), stereo/multi-mic phase correlation across 0.85x–1.15x ratios, transient pre-echo / cymbal smearing findings, and license compatibility.
4. Save benchmark JSON under `benchmarks/<engine>.json`, post results on the GitHub issue, and close it once complete.
