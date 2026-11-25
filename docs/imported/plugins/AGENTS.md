# AGENTS.md — FastTrackStudios/plugins

## Boundaries
- This repo owns **plugin UIs, parameter trees, visualizers, `fts-audio-ui`, `plugin-core`, `preset-browser`, and CLAP/VST3/standalone plugin targets**.
- Pure DSP algorithms belong in `FastTrackStudios/dsp` (`dsp` / `dsp-core`), NOT here. Do not add audio-thread DSP algorithms inside `plugin-ui` or `apps/plugins/*`.
- UI pins (`dioxus`, `blitz`, `anyrender`, `architect-ui`, `nice-plug`, `[patch]` tables) mirror `FastTrackStudios/studio` so lockfiles stay aligned.

## Verification
```bash
cargo check --workspace
cargo test -p plugin-core -p fts-audio-ui -p preset-browser
cargo test -p plugin-ui --lib
```
