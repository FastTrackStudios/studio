# plugins

FastTrackStudio audio plugin UIs, shared audio UI kit, preset browser, and CLAP/VST3/standalone targets.

Pure audio DSP engines live in [`FastTrackStudios/dsp`](https://github.com/FastTrackStudios/dsp); this repository holds everything that touches Dioxus, Blitz/Vello/WGPU, `architect-ui`, and `nice-plug`.

## Workspace Layout
- `crates/fts-audio-ui` — Shared Dioxus + Vello/WGPU audio widget kit (knobs, sliders, meters, shaders).
- `crates/plugin-core` — Shared `nice-plug` / `nice-plug-dioxus` plugin shell, editor state, meters, and tempo-sync params (`time_params`).
- `crates/preset-browser` — Preset storage, browsing, and panel UI.
- `crates/plugin-ui` — Effect-specific Dioxus views, parameter trees, visualizations, and `stack::host` adapters layered over `dsp`.
- `apps/plugins/*` — Individual CLAP/VST3/standalone plugin targets (`eq`, `eq-standalone`, `comp`, `limiter`, `gate`, `level`, `reverb`, `delay`, `modulation`, `saturate`, `mic`, `trigger`, `meter`, `pitch`, `unison`).
- `xtask/plugins` — CLAP/VST3 bundler (`fts-plugin-xtask`).

## Related Repos
- `FastTrackStudios/dsp` — Pure Rust DSP engines (zero UI/GPU/`nice-plug` deps).
- `FastTrackStudios/rhythm` — Drum/rhythm hit detection & hybrid editing extension.
- `FastTrackStudios/pitch` — F0 pitch detection, note segmentation & ARA pitch correction (`FTS Tune`).
- `FastTrackStudios/stretch` — Real-time time-stretch & pitch-shift C-ABI plugins.
- `FastTrackStudios/stem-seperation` — Embeddable multi-model stem separation library & CLI.

## Building & Bundling
```bash
cargo check --workspace
cargo run -p fts-plugin-xtask -- bundle -p eq-plugin --release --features export
just plugins-bundle "eq comp"
```
