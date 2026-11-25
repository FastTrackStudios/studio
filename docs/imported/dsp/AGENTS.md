# Agent Guide — `FastTrackStudios/dsp`

Pure Rust audio DSP workspace (`dsp-core` + `dsp`).

## Hard Rules
- **Zero UI / plugin-shell dependencies:** Never add `dioxus`, `wgpu`, `blitz`, `vello`, `architect-ui`, `nice-plug`, `fts-audio-ui`, or `plugin-core` to this repository. All UI and CLAP/VST3 shell code belongs in `FastTrackStudios/plugins`.
- **Realtime safety:** Keep audio-callback code panic-free (`unwrap`, `expect`, `panic`, allocations, locks denied by workspace Clippy lints).
- **Cargo linker quirk:** Never run `cargo` with `Cwd` inside `/run/media/Development/fasttrackstudio/studio`.
- **Verification:** Run `cargo check --workspace --all-features` and `cargo test --workspace` before committing.
