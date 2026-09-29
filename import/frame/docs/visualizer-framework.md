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

### 1. Feeds: typed data a widget can request (`features/feed`)

A **feed** is a stream with a declared shape. The catalog lists which feeds an
engine offers, and a surface subscribes with options (bins, rate, history
length, pre or post). The engine resamples to what was asked for. Reusable kinds:

| Kind | Frame payload | Options |
|---|---|---|
| `Level { channels }` | dBFS per channel | — |
| `GainReduction` | dB ≤ 0 | — |
| `Spectrum { bins, f_min, f_max, floor_db }` | dB per log-spaced bin | bins, tap (pre/post/sidechain), smoothing |
| `History { of, len }` | ring of the last `len` values of a scalar feed (GR, input level) | len, rate |
| `Waveform { len }` | samples −1..1 (a scope) | len, trigger |
| `Envelope { stages }` | stage index + position 0..1 + level (ADSR playhead per voice) | voices |
| `Scalar { range }` | one value | — |
| `Tuner` | note, cents | — |

- `feed-proto`: the kinds, `FeedInfo` (in the catalog), `FeedRequest`
  (id + options) and `FeedFrame`. Wire-compatible with `surface-proto`'s
  `StreamKind`/`StreamFrame`, which become these types (re-exported), so
  existing surfaces keep working.
- `feed-dsp`: realtime-safe producers the engines share, so every plugin
  gets the same analyser: `SpectrumTap` (windowed FFT → log bins, ballistics),
  `HistoryTap` (lock-free ring, control-thread snapshot), `LevelTap`,
  `EnvelopeTap`, `ScopeTap`. The audio thread only pushes; the control thread
  publishes at the requested rate. No allocation on the audio thread.
- `feed-memory`: demo generators for every kind (drives the mock engine,
  tests and offline renders).

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
