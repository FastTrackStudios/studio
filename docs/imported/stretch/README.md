# stretch

Swappable real-time time-stretch and pitch-shift engines behind a tiny C ABI (`daw_stretch_abi.h`), inspired by REAPER's `IReaperPitchShift`.

## Why this exists

Real-time take stretching (live scrubbing, tempo changes, stretch-marker playback) runs tight against the DAW's audio render / render-ahead path, where socket IPC per block adds latency and glitch risk. Instead of hardcoding stretch engines inside `daw` or `fts-fx`, `daw` will load engines dynamically as `.so`/`.dylib`/`.dll` plugins implementing `stretch-abi` (to be moved into `FastTrackStudios/daw` as `daw-stretch-abi`).

First-party and third-party engines — in Rust, C, C++, or Zig — implement the exact same C ABI.

## Crates

- `crates/stretch-abi`: Zero-dependency `#[repr(C)]` ABI contract + C header (`include/daw_stretch_abi.h`) + optional `host` dynamic loader (`libloading`).
- `crates/stretch-pv`: Stereo phase-locked phase vocoder (`cdylib` + `rlib`), ported from `fts-fx/features/dsp/src/stretch`.
- `crates/stretch-wsola`: Time-domain WSOLA stretcher (`cdylib` + `rlib`), ported from `fts-fx/features/dsp/src/pitch/wsola.rs`.
- `crates/stretch-host-tests`: Integration tests that `dlopen` both `.so` plugins and hot-swap between them on real audio signals.

## Prototype test

```bash
cargo build --workspace
cargo test --workspace
```
