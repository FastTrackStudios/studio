# Signal as Studio extensions

Signal (the guitar and keys rigs) in Studio's distribution of Architect
(`apps/architect`), as extensions: the rig engine a process of its own, its
screens panes any layout can place. `apps/studio/desktop` keeps hosting
Signal as it does until this replaces it.

Status of each part is in "Where it stands" at the end; this is the plan
the code follows.

## The extensions

One crate per extension under `extensions/`, each with an `extension.toml`
and a program of its own (`[entry]`), so each loads as a development copy
(`--extension <folder>`) and shadows the linked one. The linked copies are
the `rig` bundle of `apps/architect` (`studio_architect::rig()`).

| id | crate | what | views | linked |
| --- | --- | --- | --- | --- |
| `studio.rig` | `extensions/rig` | the rig engine: guitar and keys on one `signal_rig_host::Engine`, served over vox on loopback; every control as an action | none | no: a process (as `daw`) |
| `studio.rig.status` | `extensions/rig-status` | the top bar's rig status (panic, MIDI and audio lights, CPU, IN/OUT/PHONES meters, mute), the tuner, the DI player | `status`, `tuner`, `player` | yes |
| `studio.rig.switches` | `extensions/rig-switches` | the footswitch scenes | `switches` | yes |
| `studio.rig.setlist` | `extensions/rig-setlist` | the sidebar: setlist → songs → sections → stacks (and its Preset and Profile forms, by footswitch mode) | `setlist` | yes |
| `studio.rig.browser` | `extensions/rig-browser` | blocks, presets, captures on this device, TONE3000 | `browser` | yes |
| `studio.rig.grid` | `extensions/rig-grid` | the DSP grid (routing), the FX row (the selected block's faces and its preset bar) and Edit (a section's changes) | `grid`, `fx`, `edit` | yes |
| `studio.rig.macros` | `extensions/rig-macros` | the macro bar of the patch playing | `macros` | yes |
| `studio.keys` | `extensions/keys` | the keys rig's screens | `mixer`, `layers`, `routing`, `midi`, `macros`, `controller`, `piano`, `browser` | yes |
| `studio.audio-settings` | `extensions/audio-settings` | audio device, rate, buffer, routing; MIDI devices and learn; the instrument: one surface for the rig engine and the `daw` engine | `audio`, `midi`, `instrument` | yes |

`extensions/rig-link` (`studio-rig-link`) is to these what
`extensions/link` is to the arrangement and the mixer: shared at build
time, nothing at run time. It finds the engine, keeps one connection per
process, and draws a pane's rig scope (`RigPane`).

### What stays together, and why

- **Grid, FX row and Edit are one extension.** The FX row is the grid's
  selection drawn as controls, and Edit is the two side by side over the
  section's pending changes. Apart, the FX row has nothing to show. The
  selection is theirs; other extensions hear of it through actions
  (`STUDIO_RIG_GRID_SELECT` sets it, and the grid runs
  `STUDIO_RIG_BROWSER_FOCUS` through the host so a browser, if there is
  one, opens on the block's kind).
- **The status bar, the tuner and the DI player are one extension**: the
  three small readouts of the engine itself. Each is a view of its own, so
  a layout takes any of them.
- **Keys is one extension with eight views.** Its screens share one keys
  view state (`KeysRigRemote`'s) and have no panel seam yet as the guitar's
  have (`guitar::ui::panels`); they split when that seam exists.
- **The audio settings are one extension** by the owner's ask: one
  settings surface for both engines.

Everything else is apart: Switches, the sidebar, the Browser and the Macros
each work with only the engine.

## The engine

**A process of its own (`studio.rig`), not the `daw` engine's process.**

- A process has one audio mode. The `daw` engine plays a set through
  `SetAudio` with render-ahead (200 ms by default,
  `session_engine::render_ahead`): right for a song's tracks, unusable for
  an instrument played live. The rig needs the duplex engine at the
  interface's quantum (`signal_rig_host::engine::Engine::start_audio`).
- The rig engine exists and is proven headless: it is what
  `studio-desktop --engine` runs (`apps/studio/desktop/src/engine_main.rs`).
  `studio.rig` runs the same backends (`guitar::GuitarRigBackend`,
  `keys::KeysRigBackend`) on one `signal_rig_host::Engine`, so guitar and
  keys are folders of tracks in one project on one device
  (`docs/design/one-engine.md`).
- **A set's songs and a live rig play together** because each engine is a
  node of the machine's audio graph: on Linux both are PipeWire clients of
  the same interface and its clock; on macOS two CoreAudio clients of one
  device. Nothing is routed between them.
- **Where this ends up:** rigs as tracks of the set's own project
  (`one-engine.md` step 5), once the set's engine has a live bus beside its
  render-ahead one. Then `studio.rig` becomes actions and views on the
  `daw` engine and its process goes; the view extensions do not change,
  since they only hold vox clients.
- **iOS and the web have one process.** There the rig engine is in the
  app's process (iOS: `rig_engine::bootstrap_blocking`'s in-memory link;
  web: the AudioWorklet runners), and the views get the same clients from
  it. A set and a rig playing together there waits for the one-engine
  step above.

### How it is found

`studio.rig` binds `127.0.0.1` on a free port and says where in
`$XDG_RUNTIME_DIR/fts/rig-engines/<pid>.json` (`url`, `pid`, `host`: the
app that started it). A view's process takes the engine whose `host` is
its own app (itself when linked, its parent as a development copy), so two
Studios on one machine never cross. `STUDIO_RIG_URL` names an engine
outright (another machine's, `studio-desktop --engine`'s on `:4040`).

### How views reach it

As every Signal GUI does: vox clients of the engine (`RigClient`,
`RigStreamClient`, `AudioSettingsClient`, the perform pair, TONE3000's;
`KeysRigClient` and its stream). `studio_rig_link::RigPane` waits for the
connection, puts the clients in context and draws
`guitar::ui::panels::RigProviders` around the view, keyed by the
connection so a lost engine mounts it afresh. No view touches the engine's
Rust.

What panes share past the engine's own state (the grid's selection, the
browser's focus, the part being built) is a process-wide bus in
`studio-rig-link`, mirrored into each pane's context. A development copy
runs in its own process and has its own bus: it follows the engine, not
another process's selection, except where an action carries it.

### Actions

Every control is an action of `studio.rig` (it is the engine, so they run
without a window): `STUDIO_RIG_STATE`, `_START`, `_STOP`, `_PANIC`,
`_MUTE`, `_SWITCH` (press a footswitch), `_STACK` (play a stack's patch),
`_PATCH`, `_PRESET`, `_SONG`, `_SECTION`, `_MODE`, `_MACROS`, `_MACRO`,
`_BLOCKS`, `_BLOCK` (bypass, a parameter), `_BLOCK_PRESET`, `_TUNER`,
`_INSTRUMENT`, `_KEYS_*` for the keys rig, and `STUDIO_RIG_RUN` for
Signal's own command list (`FTS_SIGNAL_*`). View extensions add only what
is theirs (`STUDIO_RIG_GRID_SELECT`, `STUDIO_RIG_BROWSER_FOCUS`). Actions
reach the CLI (`studio-architect actions run`), MCP and Luau as any
extension's do.

## Audio settings

Today:

- **the rig** keeps device, input channel, rate, buffer and output routing
  in `<config>/signal/` (`sampler::RigAudioPrefs`, through the
  `AudioSettings` service), MIDI mappings in the rig library
  (`midi.styx`, `signal_rig_host::midi_learn`), the instrument in the
  rig's setup (`guitar::instrument`);
- **the `daw` engine keeps no device choice at all**: it opens the
  system's default output at the device's rate
  (`session_engine::open::open_set_audio` with
  `render_ahead::current().io_prefs()`); only render-ahead is a pref
  (`session_render_ahead`).

`studio.audio-settings` is the one surface: its `audio` view edits the
rig's prefs through `AudioSettings` and the `daw` engine's through
`DAW_AUDIO` (a new action pair of the `daw` extension: read and set device,
rate, buffer; the set's engine reopens on a change). Its values are in
Architect's settings registry (`[[sections]]`), so they show in Settings,
sync and are scriptable; the views are the rig's own Setup tabs.

## Platforms

| | desktop | web | iOS |
| --- | --- | --- | --- |
| engine | `studio.rig`, a process | in the tab (AudioWorklet) | in the app (in-memory link) |
| guitar | plays | not yet: the worklet has no guitar runner, and input needs `getUserMedia` | plays in `studio-desktop`'s iOS build; the views here once the distribution builds for iOS |
| keys | plays | plays in `studio-desktop`'s web build (`web_keys_rig.rs`); the views here once linked extensions build for wasm (`linked-core`) | not built (`signal-keys-ui`'s Vello surfaces) |
| views | linked, or a process each | linked only | linked only |

The measured gaps and their sizes are in "Web and iOS" below once item 5 is
done.

## The Rig layout

The iPad arrangement as panes: `status` across the top; `setlist` down the
left; right of it `macros` on top, `grid` in the middle, `switches` (or
`fx`, in its place) at the foot. Saved through the launcher's layout
mechanism when that lands; until then applied by `extensions/rig-dev.sh`
with `WORKSPACE_APPLY`.

## Where it stands

Filled in as each part lands (see the branch's report).
