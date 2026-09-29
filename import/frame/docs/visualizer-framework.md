# Visualizer framework: feeds, plots, curves, handles

Status: design, 2026-09-29. Goal: build signal's EQ graph, compressor
visualizer, keys-rig ADSR display and similar audio visualizers as frame
documents instead of hand-written painters. A widget asks for the data it
needs by kind, and the parts are reusable across every plugin and rig.

## The pieces

```
engine (signal / processor / plugin)            frame surface
┌──────────────────────────────┐   feeds    ┌──────────────────────────────┐
│ feed-dsp producers           │──────────▶ │ Plot nodes draw a feed       │
│  SpectrumTap, GrHistory,     │ (catalog + │  (spectrum, trace, waveform) │
│  LevelTap, EnvelopeTap, …    │  subscribe │ Curve nodes draw f(params)   │
│ registered per block         │  w/ opts)  │  (eq.response, comp.transfer,│
└──────────────────────────────┘            │   adsr.envelope)             │
          ▲ params                           │ Handle controls drag params  │
          └──────────────────────────────────│  (EQ band, ADSR breakpoint)  │
                                             └──────────────────────────────┘
```

### 1. Feeds: typed data a widget can request (`features/feed`) — built

A **feed** is a stream with a declared shape. The catalog lists which feeds an
engine offers (`Catalog::streams`, unchanged on the wire), and a surface asks
for them with options (bins, rate, history length, tap, smoothing). The
engine resamples to what was asked for. Frames carry `values: Vec<f64>`
laid out by kind (`FeedKind::frame_len`):

| Kind | Frame `values` | Options that reshape it |
|---|---|---|
| `Level { channels }` | peak dBFS per channel | — |
| `GainReduction` | `[dB]`, ≤ 0 | — |
| `Spectrum { bins, f_min, f_max, floor_db }` | dB per log-spaced bin, `f_min`→`f_max` | `bins`, `smoothing` (octaves), `tap` |
| `Tuner` | `[midi_note, cents]` | — |
| `Scalar { range }` | `[v]` in `range` units | — |
| `History { of: ScalarKind, len }` | last `len` values of `of` (`Level`, `GainReduction`, `Scalar{range}`), oldest first, one per `1/rate_hz` s | `len`, `rate_hz` |
| `Waveform { len }` | `len` samples −1..1, triggered | `len`, `trigger` (`Free`/`Rising`/`Falling`) |
| `Envelope { stages: Vec<String>, voices }` | `voices` × `[stage (−1 idle), position 0..1, level 0..1]` (`VoiceState::decode`) | `voices` |

Variant order is wire order (the five old kinds first, new kinds appended).
`Spectrum`'s band fields default (20 Hz–20 kHz, −90 dB) so old
`{"Spectrum":{"bins":64}}` JSON still reads; every other old kind
serializes byte-for-byte as before.

**`feed-proto`** (`facet` + `serde` only, no architect):
- `FeedKind`, `ScalarKind`, `FeedInfo { id, name, kind, rate_hz }`,
  `FeedOptions { bins, len, rate_hz, tap: Tap(Pre|Post|Sidechain), smoothing,
  trigger, voices }` (all `Option`, all `serde(default)`),
  `FeedRequest { id, options }` (builder: `.bins() .len() .rate() .tap()
  .smoothing() .trigger() .voices()`), `FeedFrame { id, values }`,
  `VoiceState`.
- `FeedInfo::resolve(&options)` is the one rule for "what does this request
  produce" (clamped: bins ≤ 4096, len ≤ 65536, rate ≤ 240 Hz); engines answer
  requests with it.
- It also now owns the value types feeds and params share — `EntryId`,
  `Range`, `Scale`, `Unit`, `ValueFormat`, `FormatStyle`, `note_name`, and
  `curve::{Curve, CurvePoint}` (moved from `surface-proto`, which re-exports
  all of them at the same paths).
- `surface-proto` keeps the old names as aliases: `StreamKind = FeedKind`,
  `StreamInfo = FeedInfo`, `StreamFrame = FeedFrame`, and adds
  `Catalog::feed(id)`.

**RPC** (`SurfaceEngine`, backward compatible):
`fn request_feeds(&self, requests: Vec<FeedRequest>) -> Result<Vec<FeedInfo>, SurfaceError>`
returns each feed as it will now be published on the existing `telemetry`
subscription. It has a default body (options ignored, catalog entries
returned), so engines written before feeds compile unchanged; unknown ids
give the new `SurfaceError::UnknownFeed`. Options are engine-wide (latest
request per feed wins). `surface-memory::Memory` implements it (stores the
options; `Memory::feeds()` / `frames(t)` / `tick(t)` generate each feed at its
requested shape), and the desktop player's local mock uses `Memory::frames`.

**`feed-dsp`** (depends only on `feed-proto`): each producer is a pair made
off the audio thread — a *tap* (audio thread: wait-free, lock-free, no
allocation; asserted by an allocation-counting global allocator test) and a
*reader* (control thread) that implements `FeedSource`:

| constructor | audio-thread tap | control-thread reader |
|---|---|---|
| `spectrum(fft_size, sr)` | `SpectrumTap::push / push_stereo` | `SpectrumAnalyzer`: Hann → radix-2 FFT (own, no dep) → log bins (max of FFT bins inside, interpolated when narrower) → power-domain octave smoothing → instant attack, dB/s decay, floor; `configure(SpectrumConfig)` resamples to any bin count |
| `history(cap, sr, rate_hz, Aggregate, rest)` | `HistoryTap::push(value, frames)` folds per point (`Max`/`Min`/`Mean`/`Last`) / `push_point` | `HistoryReader::snapshot` last N oldest-first, `set_rate` |
| `level(channels, sr)` | `LevelTap::push(ch, block) / push_interleaved` | `LevelReader::peak_db` (peak since last read) / `rms_db` (300 ms) |
| `scalar(Aggregate, rest)` | `ScalarTap::push(v)` (GR, LFO, CPU) | `ScalarReader::take` (extreme since last read) |
| `scope(cap)` | `ScopeTap::push` | `ScopeReader::snapshot` (latest trigger with a full window) |
| `envelope(voices)` | `EnvelopeTap::set(voice, stage, pos, level) / idle` (one `AtomicU64` per voice) | `EnvelopeReader::read_into` |

All taps share `ring()`: an overwriting SPSC ring of `AtomicU32` samples with
seqlock-style lap detection. `Publisher` holds one `FeedSource` per catalog
feed: `add(info, reader)`, `request(&[FeedRequest])` (→ resolved infos, the
body of `request_feeds`), `poll(now, emit)` emits due frames at each feed's
rate.

Engine integration recipe (processor / signal block):
1. When the block is built: `let (tap, reader) = feed_dsp::spectrum(4096, sr);`
   (and `scalar`, `history`, …); move taps into the audio processor, add
   readers to a `Publisher` with the `FeedInfo`s the block lists in its
   catalog (`publisher.infos()`).
2. Audio thread, per block: `tap.push(&block)`, `gr_tap.push(gr_db)`,
   `trace_tap.push(gr_db, frames)` — nothing else.
3. `SurfaceEngine::request_feeds` → `publisher.request(&requests)`
   (map `Err(id)` to `SurfaceError::UnknownFeed`).
4. A control-thread timer (≥ the fastest feed, e.g. 60 Hz):
   `publisher.poll(now, |f| telemetry_hub.publish(f))`.

**`feed-memory`**: pure generators of time for every kind — `frame(info, t)`,
`frames(feeds, t)`, `values(kind, rate, t)`, plus `level_db`, `gr_db`,
`spectrum_db` (pink slope, pulsing kick, gliding tone, presence bump),
`waveform`, `Adsr { … }::voice(t, v)`, and `demo_feeds()` (one feed of every
kind under `demo/`). A history is the scalar generator sampled backwards at the
feed's rate, so a GR trace ends at the GR meter's value.
`render_surface --demo-feeds [--time T]` feeds every catalog feed not named by
`--stream`/`--level` from these generators at `T`.

Not yet: the desktop player doesn't issue `request_feeds` itself (Plot
controls will, from their options); per-subscriber options would need
connection identity in the rpc.

### 2. Plots: a node that draws a feed

`ControlKind::Plot { style, x, y }`, bound to a feed:
- style: `Line`, `Area { baseline }`, `Bars`, `Steps`, `Dots`.
- x: `Linear`, `Log { f_min, f_max }` (frequency), `Time` (history);
  y: dB range or 0..1, with a `Curve`.
- The runtime turns the latest frame into the path of a `Vector` slot each
  frame, with decimation to the node's pixel width. Only plot nodes change
  per frame; everything else stays retained. Offline renders feed the plots
  with `render_surface --stream`.

### 3. Curves: a node whose geometry is a function of params

`ControlKind::Curve { provider, inputs }`: `provider` names a function
registered with the runtime (`CurveRegistry`), and `inputs` bind that function's
arguments to params (band freq/gain/q/shape, threshold/ratio/knee,
attack/decay/sustain/release).
- The runtime re-evaluates only when an input changes, then memoises the
  result.
- frame ships builtins, so surfaces work standalone:
  - `eq.response`: RBJ biquads, sum of bands; also exposes each band's curve.
  - `comp.transfer`: soft knee.
  - `adsr.envelope`
  - `filter.response`
- An engine crate may register its exact implementation under the same name
  (e.g. processor's `eq-dsp` coefficients), and that registration overrides
  the builtin.

### 4. Handles: draggable points bound to params

`ControlKind::Handle { x: Binding, y: Binding, wheel: Option<Binding>, x_map, y_map }`:
- An EQ band node: freq on a log x, gain on y, the wheel changes Q.
- An ADSR breakpoint: time on x, level on y.
- A threshold chip on a transfer curve.
- Shift gives fine control, double-click resets, and hit testing respects
  transforms.

### 5. Reusable visualizers (`surface-kit`, family `viz`)

Built from the parts above and placed like any kit component:
- `viz.spectrum`
- `viz.eq_graph`: grid, spectrum plot, band curves, combined curve, and a
  handle per band.
- `viz.comp`: input and GR history plots, transfer curve, threshold handle,
  and a ball at the input level.
- `viz.gr_history`
- `viz.adsr`: envelope curve, handles, and a playhead per voice from an
  `Envelope` feed.
- `viz.scope`
- `viz.meter_bridge`

Each is fully restylable, since each is ordinary frame nodes.

### 6. Embedding in signal

A frame surface paints into the host through the Blitz custom widget
(`Widget::paint → anyrender::Scene`) that processor's visualizers already use.
Pointer events go to the frame runtime, and the Dioxus shell stays as it is.
The wasm remote uses SVG export of the same document.

## Order of work
1. frame: feeds contract + mock generators + plots + curves + handles + viz kit
   (this repo), with examples: comp visualizer, EQ graph, ADSR.
2. processor: `feed-dsp` adoption in the comp / EQ blocks, and exact curve
   providers registered from `eq-dsp` / `comp-dsp`.
3. signal: engine publishes feeds per block, and a frame-surface Blitz widget
   replaces `CompViz`, `EqGraph` and the keys ADSR panels.

## 7. Live reload: agents edit, the running app updates

Status: push reload built. `frame --library ws://… --surface ID` follows a
surface in `SurfaceStore`: updates are whole snapshots (see `library-proto`
for why not deltas), swapped in with `Runtime::swap_surface`, and gated by
parse and lint (refused with a toast). `frame-mcp --library` publishes after
every edit. The Loro multi-editor merge below is not built yet.

Nothing requires a relaunch. The document is the unit of change, and every
host re-renders when it changes:
- **Today:** the desktop player polls its `.fm` file and reloads it, and
  `frame-mcp --file` autosaves after every tool call. So an agent editing
  through MCP updates the open window within about a frame.
- **Push instead of poll:**
  - A `SurfaceStore` rpc (below) streams document updates, as Loro deltas,
    to every host that has the surface open: the desktop player, the frame
    widget embedded in signal, and a browser.
  - The runtime swaps scenes in place and keeps control state: values,
    gestures in flight, meter ballistics, plot rings.
  - An update that fails to parse or lint is rejected and reported. It never
    blanks a live stage.
- **Several editors at once:** agents and people edit the same Loro document
  (CRDT), so edits merge instead of overwriting.
- **Scope:** node ids and bindings are stable, so a reload doesn't drop
  MIDI/param mappings.

## 8. Surface library: browse, share, download

Status: `library-proto`, `library-fs` (a watched folder), `library-link`
(WebSocket serve/connect, `Follow`), the `frame-library-server` app (vox plus
HTTP index, downloads and previews) and the desktop browser (`frame
--library … --browse`: grid, fits filter, preview live, install, fork) are
built. Ratings, a community index and the WGSL sandbox are not.

A feature (`features/library`) for storing and finding surfaces:
- `library-proto`: `SurfaceStore` rpc.
  - `list(query)`, `get(id, version)`, `publish(bundle)` and
    `subscribe(id)` (live updates for §7).
  - Metadata: name, author, description, tags, the plugin or rig it fits (by
    catalog signature, so a surface only offers itself where its bindings
    resolve), preview PNG, version, license, and forked-from.
- Backends:
  - `library-fs`: a folder of `.frame` bundles. Local, and what the desktop
    player serves.
  - `library-server`: a shared community index over HTTP/vox with
    publish, versions, ratings and downloads.
- Hosts:
  - The desktop player gets a browser view: previews, filter by "fits this
    plugin", one-click install or preview live against the running engine,
    and fork to edit.
  - Signal lists the installed surfaces per plugin or rig and switches between
    them live.
- Safety: surfaces are data. Custom WGSL (`asset:` shaders) is validated by
  naga and sandboxed to the effect contract, and anything that fails falls
  back to plain drawing.
