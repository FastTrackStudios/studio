# studio-core: what both sides share

Data and contracts that the UI (`studio-connect`) and the engine
(`studio-engine`) both need, so neither has to depend on the other. It is
the bottom of the graph. Keep it **small**: types, formats and their pure
logic.

| Folder | What it holds | Came from |
|---|---|---|
| `proto/` | the UI↔engine contract: `daw-proto`, `daw-edit`, `daw-view`, `daw-timeline`, `daw-module`, `studio-proto`, Signal's and Ignition's protos | daw, signal, Ignition |
| `session/` | the `.session` format (one Loro document per song) and its file I/O: `session`, `session-sync`, `session-format`, `song`, `song-file`, `library-sync`, the Keyflow and Ignition lanes | session, studio |
| `music/` | `music-convention`, `monarchy`, `lyrics` (the lyrics model), Keyflow (`keyflow`, `keyflow-lang`, `keyflow-proto`, `engraver`, `tree-sitter-keyflow`) | music-convention, keyflow |
| `frame/` | `frame` (the `.frame` format and renderer), `frame-doc` | frame |
| `util/` | `utils`, `fts-version`, `fts-account` (sign-in, the kept session, org sync; its `ui` feature is the sign-in panel, to move out to studio-connect) | daw, studio |

## Rules

- **Depend on core only** (and `vendor/`): never on a connect or engine
  crate, and never on Dioxus, Blitz or app-shell. `just boundaries`
  checks both. Today's exceptions are in the allow-list at the end of
  `boundaries.toml`; they only get removed.
- **No audio thread here, and no UI.** A type that needs an engine to mean
  anything belongs in `studio-engine`; a component belongs in
  `studio-connect`. If you want `daw::` or `dioxus::` in a core crate, the
  code is in the wrong group.
- **The session knows nothing of its extensions.** A field only one
  extension understands lives in that extension's data. Project state is
  the Loro document; playback sync is `session`'s `sync`. Never put a
  playhead in the document.
- **Keyflow owns the musical primitives**: `TimeSignature`,
  `MusicalPosition`, `Tempo` and `TimeRange` are `keyflow-proto`'s; the DAW
  reads them from there. `keyflow` and `engraver` are the public surfaces.
- **`ChartPipeline` is the one way to engrave a chart**
  (`engraver::api::pipeline::ChartPipeline::shared()`): fonts, layout,
  presets and the SVG/PDF exports all go through it.
- **Frame's core links no architect, vox, Dioxus, figcraft or crdt** (a
  deny row in `boundaries.toml`); what needs architect goes in
  `frame-architect`.
- **wasm-clean**: core crates are in the browser builds (`just check-web`).
  `cargo check --workspace` builds for the host and never sees a
  `cfg(target_arch = "wasm32")` block. Two traps that compile and then
  fail: `tokio::time::sleep` panics in a browser (use
  `architect::platform::sleep`), and `getrandom` 0.3 needs both the
  `wasm_js` cfg and the `wasm_js` feature of a direct dependency.
- **Fixtures inside the crate that reads them** (`<crate>/tests/fixtures/`).
  Keyflow's corpus tests read corpora a clean clone lacks, so they are
  `#[ignore]`d (`docs/imported/keyflow/AGENTS.md`, "The corpus tests").

## Build and test

```bash
just test core     # every core crate's tests
just check-web     # the browser build, which core is in
just boundaries    # the group rule, the promise, frame's deny row
```

The repos this came from: `docs/imported/{session,keyflow,frame,music-convention,daw}/`.
