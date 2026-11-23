# rhythm

Swappable hit detection, rhythm analysis, and hybrid slip/stretch drum editing as an out-of-process DAW extension (`fts.rhythm`).

## Why this exists

- Keeps heavy multi-mic hit detection, corpus benchmarks, and drum-editing orchestration out of `fts-fx` and `session-engine`.
- Rebuilds in seconds (`cargo build -p rhythm-ext`) and hot-swaps against a running DAW without recompiling Studio.
- Dogfoods the same public extension contract third-party rhythm / transient detectors use.

## Crates

- `crates/rhythm-hits`: Multi-mic calibrated kit hit detection (ported from `fts-fx/features/dsp/src/hits`) behind a swappable `HitDetector` registry (`fts.hits.kit-fusion`, `fts.hits.envelope-gate`).
- `crates/rhythm-editing`: Pure, DAW-free per-hit placement and hybrid slip-vs-stretch planner (prototype for `daw-editing` in `FastTrackStudios/daw`). Supports moving individual hits (UI drag or agent) while keeping attacks natural and choosing per-seam between slip+crossfade, transient-protected tail-stretch, and warp.
- `crates/rhythm-ext`: Process extension (`fts-rhythm` binary + `extension.toml`) with stdio JSON-RPC server and CLI.

## Prototype test

```bash
cargo test --workspace
```
