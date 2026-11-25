# studio-engine: the DAW engine

The full DAW, headless: it records, edits, mixes and renders a song, runs
the rigs, analyses recordings and drives the lights. Engine work never
compiles or waits on the UI. `just engine` runs it alone.

| Folder | What it holds |
|---|---|
| `engine/` | `studio-engine` (the facade: the set, the Action List, the tools), `engine-runtime`, `engine-host`, `engine-sync` |
| `daw/` | the base: `daw`, `audio`, `midi`, `fts-sample`, `daw-standalone`, `daw-control`, `daw-sync`, the plugin host, the audio graph |
| `dsp/` | `dsp`, `dsp-core`, the effects and their models, stretch, `plugin-processors` |
| `signal/` | Signal: the sampler, NAM, the rigs (guitar, keys, drums, synth, orchestra, …) |
| `analysis/` | rhythm (hits, editing), pitch, stem separation, tempo maps, Keyflow's DAW analysis |
| `lighting/` | Ignition's engine: DMX, fixtures, effects, playback, show control |
| `adapters/` | the hosts and formats: REAPER, Ableton, Logic, Pro Tools, control surfaces, `dynamic-template` |
| `extensions/` | `daw-extension` (the engine's program today), pitch, rhythm, tempo map |
| `plugins/` | the CLAP/VST3 bundles (the FX, guide, nam, signal, the sampler) and `verify/` |

## Rules

- **Depend on core and engine only, and never on Dioxus, Blitz, app-shell
  or a studio-connect crate.** The exception is `plugins/*`, which may use
  `studio-connect/plugin-ui/*`. A rig's or a tool's UI belongs in
  studio-connect (`views/`), on the other side of the protocol.
  `just boundaries` checks both rules. Today's violators are the
  allow-list at the end of `boundaries.toml`, and each one comes off as it
  is untangled.
- **One engine per process.** One `Standalone`, one project, one realtime
  audio engine on one device. A rig is a folder of tracks in that project
  (`signal_rig_host::engine::{Engine, Rig}`), not an engine of its own. Read
  `docs/design/one-engine.md` before touching rigs, engines, meters or MIDI.
- **Detachable GUI**: every GUI (desktop, phone, browser, plugin editor) is
  a client of the headless engine.
- **Processing cores build for native, wasm (AudioWorklet) and embedded**:
  the audio graph, the effects, the sampler engine. No heap allocation on
  the hot path (allocate in `reset()`; `process()` never pushes or boxes),
  no threads or task spawning, no platform I/O (cpal, web-sys and MIDI
  drivers live in adapters), and keep `AudioNode: Send`.
- **Realtime lints**: `daw/audio/clippy.toml` and `signal/rigs/clippy.toml`
  add the audio-thread rules (locks, `env::var`, sleeps). Clippy reads the
  nearest one. Keep the copies identical.
- **Effects depend on no product**: nothing in `dsp/` depends on a rig, an
  adapter or the app. If an effect seems to need one, the product adapts to
  the effect instead.
- **A parameter parses the string it prints** (each effect's
  `tests/param_round_trip.rs`). Tempo-synced times use `dsp_core::time`.
- **Models and audio are never committed** (`.onnx`, `.pt`, `.safetensors`,
  `.wav`). Analysis models are cached under `FTS_RHYTHM_MODEL_DIR` /
  `FTS_PITCH_MODEL_DIR` (default `~/.cache/fts-*/models`). A non-commercial
  model is an optional download (`"tier": "non_commercial"` in
  `models.json`), never a default. `ort` is 2.0.0-rc.15 with
  `load-dynamic`: point `ORT_DYLIB_PATH` at `libonnxruntime.so`.
- **Lighting**: load `ignition-domain`, then `ignition-program` and
  `ignition-program-song` (`.claude/skills/`) before writing a cue. Name a
  library entry instead of typing its values, and write roles, never
  fixtures, in a show. `just spec-coverage --scope lighting` checks the
  spec tags (CI does).
- **Timing tests** (the engine's sync and render-ahead tests) have
  wall-clock budgets: they fail on a loaded machine and pass alone.

## Build and test

```bash
just engine            # build the engine and run it headless
just test engine       # every engine crate's tests (the REAPER suites are skipped)
just plugins-bundle    # the bundles into target/bundled; `just plugins-verify` loads each
```

The repos this came from: `docs/imported/{daw,dsp,plugins,signal,rhythm,pitch,stretch,Ignition,studio-adapters,keyflow}/`.
