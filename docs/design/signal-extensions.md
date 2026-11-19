# Signal as Studio extensions

Signal (the guitar and keys rigs) in Studio's distribution of Architect
(`apps/architect`), as extensions: the rig engine a process of its own, its
screens panes any layout can place. `apps/studio/desktop` keeps hosting
Signal as it does until this replaces it.

What was built and how it was checked is in "Where it stands" at the
end.

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
| `studio.keys` | `extensions/keys` | the keys rig's screens | `mixer`, `layers`, `routing`, `midi`, `macros`, `controller`, `piano`, `browser`, `switches` | yes |
| `studio.audio-settings` | `extensions/audio-settings` | audio device, rate, buffer, routing; MIDI devices and learn; the instrument: one surface for the rig engine and the `daw` engine | `settings` (the page in Settings), `audio`, `midi`, `instrument` | yes |

`extensions/rig-link` (`studio-rig-link`) is to these what
`extensions/link` is to the arrangement and the mixer: shared at build
time, nothing at run time. It finds the engine, keeps one connection per
process, and draws a pane's rig scope (`RigPane`).

### What stays together, and why

- **Grid, FX row and Edit are one extension.** The FX row is the grid's
  selection drawn as controls, and Edit is the two side by side over the
  section's pending changes. Apart, the FX row has nothing to show. The
  selection is theirs; `STUDIO_RIG_GRID_SELECT` sets it by action, and a press that
  asks for the browser (a face's preset name, a part's "Patch…") opens
  the browser's view through the host (`EXT_VIEW_OPEN`), if there is one.
- **The status bar, the tuner and the DI player are one extension**: the
  three small readouts of the engine itself. Each is a view of its own, so
  a layout takes any of them.
- **Keys is one extension with nine views.** Its screens share one keys
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
`KeysRigClient` and its stream). A Signal view sleeps and spawns through
tokio, and Architect's window has no tokio runtime on its thread, so a
rig pane's thread enters the link's own, once. `studio_rig_link::RigPane` waits for the
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
without a window; 44 of them, `extensions/rig/extension.toml`):

- the engine: `STUDIO_RIG_STATE`, `_START`, `_STOP`, `_RESTART`, `_PANIC`,
  `_MUTE`, `_RENDER` (a profile's patches rendered offline, on no device);
- playing: `_MODE`, `_SWITCH` (a footswitch tapped or held), `_STACK`,
  `_PATCH`, `_PRESET`, `_SONG`, `_SECTION`, `_SETLIST`, `_PROFILE`, `_TAP`,
  `_TUNER`;
- the sound: `_MACROS`, `_MACRO`, `_BLOCKS`, `_NODES`, `_BLOCK` (bypass, a
  parameter, an option), `_BLOCK_PRESET` (a block's or a module's);
- reading: `_PERFORMANCE`, `_PATCHES`, `_PRESETS`, `_LIBRARY`;
- setup: `_INSTRUMENT`, `_SETUP`, `_AUDIO`, `_MIDI` (ports, learn);
- keys: `_KEYS_STATE`, `_KEYS_START`, `_KEYS_STOP`, `_KEYS_PRESETS`,
  `_KEYS_PRESET`, `_KEYS_STACK`, `_KEYS_MIXER`, `_KEYS_MACROS`,
  `_KEYS_MACRO`, `_KEYS_NOTE`, `_KEYS_MIDI`;
- `STUDIO_RIG_RUN`: Signal's own command list (`FTS_SIGNAL_*`).

A view extension adds only what is its own: each a `…_STATE`,
`STUDIO_RIG_GRID_SELECT` (the selection), `STUDIO_RIG_BROWSER_FOCUS`,
`STUDIO_KEYS_OPEN` (an engine or a layer), and
`STUDIO_AUDIO_SETTINGS_STATE` / `_SET`. Actions reach the CLI
(`studio-architect actions run`), MCP and Luau as any extension's do.

## Audio settings

What there was:

- **the rig** keeps device, input channel, rate, buffer and output routing
  per audio rig in `<config>/signal/rigs/<rig>.styx`
  (`sampler::rig_manager::RigManager`, read and written through the
  `AudioSettings` service and the rig's Setup), MIDI mappings in the rig's
  own `midi-learn.styx` (`signal_rig_host::midi_learn`), the instrument in
  `<config>/signal/instrument` (`guitar::instrument`);
- **the `daw` engine kept no device choice at all**: it opened the
  system's default output at the device's rate
  (`session_engine::open::open_set_audio`), only the render ahead a pref.
  Session's own Setup page says as much ("the desktop engine's device
  routing is next").

What there is now:

- the `daw` engine keeps an output device, sample rate and buffer
  (`session_engine::audio_device`, `~/.config/fts/audio-device.json`,
  `SESSION_AUDIO_DEVICE`; the `daw` extension's `DAW_AUDIO` adds the
  outputs this machine has) and reopens its device on a change;
- `studio.audio-settings` is the one surface: each engine's device, rate
  and buffer side by side (the rig's input and channel too), then the
  rig's own Setup pages (its interface, outputs and test tones; its MIDI
  controllers, with what each switch is learned to; the guitars, their
  tone, gates and level). It is a section of Architect's Settings
  (`[[sections]]`, view `settings`) and three panes (`audio`, `midi`,
  `instrument`);
- one value is in Architect's settings registry:
  `studio.audio-settings.link_engines` (on by default). With it on, a
  change to the rig's output device or sample rate is made to the `daw`
  engine too, so both play through one interface at one rate. The buffer
  stays each engine's own: the rig plays live at a small one, the `daw`
  engine renders ahead. The engines' own values stay in the engines'
  stores, where each engine reads them as it starts with no window open;
- as actions: `STUDIO_AUDIO_SETTINGS_STATE` and `STUDIO_AUDIO_SETTINGS_SET`
  (`engine`: `studio`, `rig` or `both`).

What it does not do yet: the guitar rig takes every MIDI port (it has no
port choice to keep; the keys rig has one, `STUDIO_RIG_KEYS_MIDI`), and the
rig's Audio page lists the interfaces Signal knows by name rather than the
devices found (Signal's own screen; the engines' rows above it list what
is found).

## Platforms

| | desktop | web | iOS |
| --- | --- | --- | --- |
| engine | `studio.rig`, a process | in the tab: the AudioWorklet runners (`features/adapters/web/worklet`: keys and guitar) | in the app: the rigs on an in-memory link (`LocalServer`) |
| guitar | plays | the DSP runs in a worklet with live input (its harness page); no rig service over it, so no view can drive it | plays in `studio-desktop`'s iOS build |
| keys | plays | plays in `studio-desktop`'s web build (`web_keys_rig.rs`) | switched off in `studio-desktop` (`rigs.rs`); its phone screen exists |
| views here | linked, or a process each | not yet: Studio's distribution has no web build | not yet: Studio's distribution has no iOS build |

### Web and iOS: what stands between this and playing there

Sizes are estimates from reading the code, not from builds.

**Both.** `studio-rig-link` finds an engine by a file and dials a
WebSocket. A tab and an iOS app have one process, so it needs a second way
in: the rigs' router served in-process (`architect::LocalServer`) and its
clients handed to the panes (`pool().caller_with`, or
`LocalServer::establish`). `extensions/rig`'s `router()` is that router
already; its `serve()` is the loopback part. About 100 to 150 lines and a
few days, once there is a build to try it in.

**Web.**

1. *Studio's distribution has no web build* (`apps/architect` is
   `architect-desktop` only; its web bundle is Architect's stock wasm plus
   a `distribution.json`). It needs a `web` lib over `architect_web::run`,
   `guest` behind a target cfg in the nine rig crates (a browser has no
   processes), and every linked crate building for wasm32, which is
   untried for the `stage` bundle (`session-engine/native`). Days for the
   crate setup; days to weeks for `stage`.
2. *Guitar has no rig service in the tab.* Its DSP is there
   (`worklet/src/guitar`: the chain runner, NAM on workers over shared
   memory, input from `getUserMedia` with processing off), and the guitar
   crate builds for wasm32 with the backend compiled out. What is missing
   is what `web_keys_backend.rs` is for keys: the `Rig`, `RigStream` and
   `AudioSettings` services over the worklet, served with `LocalServer`.
   One to two weeks; and a build recipe for the guitar worklet (about 40
   lines, none exists).
3. *The DSP grid and the block faces draw nothing on wasm*
   (`routing_canvas.rs` is a Blitz widget painting a Vello scene; on wasm
   it is an empty `div`, as the faces are). The scene code lifted onto a
   WebGL canvas as `session-ui`'s `web_host.rs` does it: about a week, and
   the faces again as much.
4. *The keys rig in the tab lives in `studio-desktop`'s app crate*
   (`web_keys_*.rs`, `web_packs.rs`, about 6 000 lines) and takes its
   program and packs from an engine it can reach. Moving it to a library:
   days. Where an Architect page gets packs from is undecided.
5. *Cross-origin isolation.* The guitar's worker NAM and the keys'
   threads need `SharedArrayBuffer`; Architect's web host does not send
   COOP/COEP. Without them guitar runs its models inline and keys runs on
   one thread. A small change in Architect.
6. *Input permission and latency.* The browser asks for the microphone
   on a press (`getUserMedia`); measured so far is the guitar's render
   cost (about 310 µs a 2.7 ms quantum, Chrome on an M-series Mac) and the
   keys' output latency (13.7 ms base plus 32 to 45 ms at `interactive`;
   `latencyHint` 0.005 is the default now). The input round trip is not
   measured anywhere, nor Safari. Days to measure.

**iOS.**

1. *Studio's distribution has no iOS build* (`architect_mobile::run`,
   `cargo xtask dist ios`): `architect-desktop` moved to non-iOS, an
   `Info.plist` with the microphone string
   (`apps/studio/desktop/ios/app-plist.sh` has it), and every linked crate
   building for `aarch64-apple-ios`; `setlists`' PDF writer is known not
   to (studio#492). Days, to weeks with `stage`.
2. *The engine in the app.* Processes never run on iOS, so `studio.rig`
   becomes a linked extension there that serves `router()` in-process,
   with `studio-desktop`'s audio session (`ios_audio.rs`: play and record,
   measurement mode, 128 frames at 48 kHz, the external input only, the
   record permission asked) and its external-input watcher
   (`rig_engine.rs`) moved out of the app crate. About a week.
3. *Keys.* `studio-desktop` greys the keys tile out on iOS with no reason
   given in the code; its comments blame Vello/baseview surfaces, but the
   iOS build already compiles `keys` with `ui`. Whether `studio-keys`
   builds for iOS is one build on the Mac; why keys "does not work" there
   needs the device. Unknown until tried.
4. *Latency.* 128 frames at 48 kHz is asked for (2.7 ms a buffer); no
   measured round trip is written down.

None of these was small enough to fix and prove on this branch; the one
correction made is to what was believed: the web already has a guitar
runner, and it is the rig service over it that is missing.

## The Rig layout

The iPad's screen as panes (`extensions/rig-dev.sh` opens and arranges
them with `WORKSPACE_APPLY`):

```
guitar                                      keys
+------------------------------------+     +----------+-----------------------+
| status                         9 % |     |          | mixer            46 % |
+---------+--------------------------+     | browser  +-----------------------+
|         | macros              14 % |     |          | layers           30 % |
| setlist +--------------------------+     |   22 %   +-----------------------+
|         | grid | fx | edit |       |     |          | switches | piano |    |
|  24 %   | browser             48 % |     |          | macros | routing |    |
|         +--------------------------+     |          | midi             24 % |
|         | switches | tuner    38 % |     +----------+-----------------------+
+---------+--------------------------+
```

Studio's launcher (`feat/project-launcher`, `STUDIO_LAUNCHER_LAYOUT_SAVE`)
is not in this branch's base; when it lands this is the layout to save
through it as "Rig".

## Where it stands (2026-11-18)

Verified in an isolated instance (`studio-architect dev`, a headless
compositor, a copy of the rig library, its own config folder):

- **Guitar, desktop.** `studio.rig` loads the library and serves it; the
  status bar, the sidebar (preset, profile and setlist forms), the
  browser, the DSP grid, the FX row, Edit, the macros, the switches and
  the tuner draw as panes and take the pointer; a stack switched, a macro
  set, a block selected and a song, section and mode changed by action
  show in the panes. Each view extension also passes `EXT_CHECK` as a
  process package, and the switches were run as a development copy.
- **Audio.** No real device was opened. The rig's patches were rendered
  offline in the engine's process (`STUDIO_RIG_RENDER`: all 26 of a
  profile's patches built and measured). The live path opened its stream
  on a private PipeWire holding only null devices and built its chain
  (22 patches, none failed); with no session manager there the graph
  never ran, so no real-time cycle was measured.
- **Audio settings.** Both engines' device, rate and buffer set by action
  and read back from their stores, the `daw` engine following the rig's
  output when linked; the page drawn. Not tried: the `daw` engine
  reopening a real device with a set playing.
- **Keys.** The keys rig starts in the engine (its profile, 13 layers, the
  pack library read) and the mixer pane draws with its keyboard and
  controller strip; the other eight views render (`EXT_CHECK`) and were
  not each looked at.
- **Not done:** a drive pedal's face in the FX row is its name only (the
  effects' faces draw: the compressor's was looked at, with `just faces`'
  output in place); TONE3000's sign-in callback is `studio-desktop`'s and is not served by
  `studio.rig`; a development copy does not follow another process's
  selection.
