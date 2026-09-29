# The hardware kit — conventions

`surface-kit` is frame's premade component library: controls in the style of
real audio hardware, ported from FTS's `fts-audio-ui::hardware` kit
(`processor/libs/ui/fts-audio-ui/src/hardware`). Agents list them
(`surface_kit::components()`, the `components` tool) and place them by id
(`surface_kit::place`, the `add_component` tool).

## What a component is

A `BuildFn(&mut Builder, parent, &Place) -> NodeId` that inserts **plain frame
nodes** (frames, ellipses, rectangles, vectors, text) under `parent` and
registers a `surface_proto::Control` on the root (`b.controls.insert(root, …)`)
whose **slots** point at the moving parts:

| role | used for |
|---|---|
| `Needle` | anything that rotates with the value (a knob's turning body, a VU needle). Rotates about the **centre of its parent** — so put it inside a square frame centred on the pivot. Draw it at the **centre of travel** (pointing up for a standard 300° knob). A `Frame` in this slot turns its whole subtree (text included), so a knob's body is a square frame filling the knob holding everything that turns with the shaft: teeth, knurl, index, wing/beak and their shadows, numerals on the skirt. (A `Group`'s own transform is *not* inherited — rotating containers must be frames.) |
| `ValueArc` | an arc drawn from start of travel to the value |
| `Fill` / `Thumb` / `PeakHold` | bars and sliders (drawn at full extent / top). On a `Fader`/`Meter` they move along its orientation; on a `Gauge` (no orientation) a `Fill` grows along its own long side and a `Thumb` travels along its lane's — so a wide lane slides sideways |
| `Value` / `Label` | readout text / legend text (an authored legend wins). `Control::format` (`ValueFormat`: decimals, unit, note name, one label per step) says how `Value` prints |
| `Led` | lamps: full opacity when on, 20% (and no emission) when off (for `ButtonMode::Select` banks, when *its* option is selected). On a `Gauge`/`Meter` the `Led`s are a **ladder**: lamp `i` of `n` (slot order) lights once the reading reaches position `(i + 1) / n` — the binding's curve puts each printed threshold there |
| `Glow` | brightness follows the value: opacity = position (so its emission scales too) — a lamp on a scalar stream |
| `Trace` / `Area` | a `Plot`'s or `Curve`'s line (stroke it, `align` centre) / the region between it and the baseline (fill it, a top → bottom gradient): the runtime rewrites the vector's geometry, decimated to its width, in the node's own box — make them the plot area's size |
| `Marker` | a node centred on a `Curve` at its `marker` input (the compressor's input ball); hidden without a reading |

A `Handle` needs no slot: its node (a touch-sized frame, the dot drawn
centred in it) is moved to its point in its **parent**, which is the area
its axes span.

Nothing else is special. A placed component can be restyled or taken apart
like any other nodes.

## Placing: what `Place` says

| field | effect |
|---|---|
| `ink` | `Ink::Light` (default, each component's own silkscreen), `Ink::Dark` (`DARK_INK`, for grey/aluminium panels) or `Ink::Color("#hex")`: every printed scale, numeral, detent, tick and panel legend. Legends printed *on* a cap keep the cap's ink; VU cards print in the card's ink. |
| `keep_case` | legends, options and captions as written (`"kHz"`, `"Trim dB"`) instead of capitals |
| `sweep` | travel in degrees for knobs, rotary switches and levers. Without it a knob given `options` is **stepped**: it prints the options as its scale and turns ≤ 60° a detent, ≤ 300° in all (`detent_sweep`); a rotary switch the same; a lever ±62° |
| `touch` | a **tight box**: the control's box (hit target) is the moving part — the knob body, the dial, the paddle's reach — padded out to at least this edge (`TOUCH_MIN` = 44 for the lint), centred on the shaft. The printed scale and legend stay where they were and hang outside it. `None` keeps the full drawn box, which auto layout needs; to lay tight components out, `place_in_cell` wraps one in a holder the size of its drawing (`drawn_bounds`) |
| `reverse` | banks, push-in columns, LED rows and ladders run last option first (a column whose top is the param's last option; a GR ladder lit right to left) |
| `inner` | the cap of a concentric pair (`knob.concentric`): its binding, legend, sweep |
| `select` | a meter's source switched by a choice param (`SourceSelect` of `MeterTap { stream, channel, offset_db }`, `None` = off): the 1176's GR / +4 / +8 / OFF. Bind the default stream too |
| `format` | the control's `ValueFormat` |
| `options` | also carries `key=value` settings and flags for components configured that way (`panel.*`: `w=`, `h=`; LED meters: `floor=`, `ceiling=`, `segments=`, `gr`, `vertical`; lamps, LED meters, footswitch LEDs: `color=#hex`, or per lamp `colors=#a,#b,…` (`Place::lamp_color`); footswitch and single buttons: `momentary`; VU meters: `glass`, a glass shader over the window on GPU renderers) — `Place::opt`, `Place::flag` |

Builders read these through `Place::ink_color(light)`, `Place::case()`, and
`Place::control(kind)` (a `Control` carrying the binding, format and
select), and end with `registry::tighten` when `touch` is set.

## Rules

- **Ids**: `family.style` — `knob.daka`, `knob.marconi`, `meter.vu_ivory`,
  `button.square_lit`, `switch.toggle_bat`, `panel.brushed`. Snake case.
- **Size**: `Place::size` is the primary dimension (knob diameter, meter width,
  button edge). Default to `ComponentDef::default_size`. Everything scales from
  it — never hard-code pixel sizes that don't.
- **Variants**: colours/finishes as `variants` (`["black", "red", "blue", "grey"]`).
  Unknown variants are rejected by `place` before your builder runs.
- **Legend + scale**: honour `Place::legend` and `Place::scale` (printed scale
  labels, first = start of travel) where the style prints them; otherwise use
  the source kit's defaults for that style.
- **Binding**: pass `Place::binding` into the control unchanged. (One
  exception: a needle meter whose printed scale isn't linear in dB — a VU
  card — swaps a *linear* stream binding's `curve` for its scale law, which
  the runtime's `Gauge` applies over `floor_db..ceiling_db`, so the needle
  lands on the printed marks. See `meters::scale`.)
  `ComponentDef::binds` says what it expects: `"param"`, `"choice param"`,
  `"toggle param"`, `"level stream"`, `"action"`, `"none"` (panels).
- **Kinds**: knobs → `ControlKind::Knob { sweep_degrees }` (use the style's real
  travel, or `Place::sweep`); needle meters → `ControlKind::Gauge { sweep_degrees, floor_db, ceiling_db }`;
  LED bars and ladders → a `Gauge` with no sweep whose `Led`s are the lamps
  (`floor > ceiling` reads a gain-reduction span, 0 dB first); bar meters →
  `ControlKind::Meter`; buttons → `ControlKind::Button { mode }`
  (Toggle / Momentary / Trigger / `Select { index, count }` for banks);
  footswitches → `Footswitch` (or `Button { Momentary }`); faders/levers →
  `Fader`; rotary selectors and paddle levers → `Knob` over a choice param
  (stepped), their pointer/paddle a `Needle` frame in a square centred on the
  pivot so it turns as one body; stream lamps → `Readout` with a `Glow`.
- **Concentric pairs**: the inner control's node sits *inside* the outer
  control's node and box (`knob.concentric`). `design-lint` reads that
  nesting as a declared pair, not an overlap, and the runtime's hit test
  gives the inner (painted later) its box and the outer the rest.
- **Meters at rest**: a level stream rests at silence (the floor); a
  gain-reduction stream rests at **0 dB** (a GR needle on a VU card sits on
  0 VU) and attacks *downward*, recovering at the release rate. Readings of
  either sign count as reduction. Author lamps and covers in their rest
  state so a still render with no data reads right.
- **0 VU = −18 dBFS**: every needle VU sets `Control::reference_dbfs` to
  `surface_proto::VU_REFERENCE_DBFS`, so a level stream at −18 dBFS reads
  0 VU. Gain reduction ignores it (already relative to unity). A meter
  switch's `+4`/`+8` ranges are `MeterTap::offset(-4.0 / -8.0)` on top.
  LED bars, columns and ladders stay in dBFS.
- **Paint** with `crate::draw` helpers: `linear(angle, stops)` (CSS angles),
  `radial_at(cx, cy, r, stops)` (unit-square hot-spot), `conic(start, stops)`
  (brushed metal), `drop_shadow`/`inner_shadow`, geometry `polar`, `sector`,
  `lobed`, `regular`, `polygon`, `rotated`. Angles: degrees, 0 = up, clockwise.
- **What turns and what doesn't**: the room's light doesn't rotate with a
  knob. Keep lit shading (off-centre gradients, highlights, rim light,
  directional shadows) *outside* the `Needle` frame — round tiers under it,
  highlights in a fixed overlay above it — and paint turning parts with
  rotation-invariant fills (flat, or radial about the shaft). A turning
  part's shadow has no offset (it would swing round with it): give the
  part's outline one zero-offset `drop_shadow` — the renderer shadows a
  vector along its real outline, so it turns with the part.
- **Light**: `draw::emissive(node, color, intensity, radius)` sets
  `Node::emission` where real hardware glows (lamps, jewels, backlit caps,
  LEDs, a lit meter card or needle). Put it on the node an `Led` slot dims,
  so the light goes out with the lamp; keep intensities tasteful (≈0.3–0.9).
  It's optional — a component must read without an effects stage.
- **Descriptions** are for a model choosing a component: what it looks like,
  which unit it's from, when to use it. One or two sentences.

## Visualizers (`viz.*`)

Graphs, not hardware: `viz.spectrum`, `viz.gr_history`, `viz.scope`
(one feed each, bound by `Place::binding`) and `viz.eq_graph`, `viz.comp`,
`viz.adsr`, which find their params by namespace (`options: ns=<ns>`) on a
fixed pattern — `surface_kit::viz::catalog` builds the params and feeds a
placed component expects, for engines and mock catalogs alike.

- **Structure**: a panel; a *well* (the lit plot background); a *plot
  area* frame holding the grid and the handles (unclipped, so a handle at
  an edge shows whole); inside it a clipped *traces* frame holding one
  area-sized frame per `Plot` / `Curve` control with its `Area` then
  `Trace` vectors. Labels are text nodes printed round the area.
- **Look**: soft grid (`#ffffff0f`, majors `#ffffff21`), labels `#8e98a8`
  at 9 px, a lit line (`draw::emissive`) over a gradient region; band
  colours from `viz::BAND_COLORS`.
- **Handles**: a 44 px frame (the touch box) with the dot centred in it.
  `design-lint` doesn't check handles for overlap (they move with their
  values), and needs their parent to have a size (`handle-area`).
- **Curves** share one evaluation when their provider and inputs match —
  give every band curve of an EQ all the bands' inputs and pick its own
  with `series`.
## Motion

Parts that move by themselves carry `Node::motion` (see
`docs/visualizer-framework.md` §9). `surface_kit::motion` has the presets —
`spring_follow(param)`, `spring_lag(input, target, travel)` (physics on a part
a control already moves), `ring_on(level_feed, freq, decay)`,
`spin(turns_per_second)`, `flicker(amount)`, `pulse(rate, low)` — and
`motion::add(b, node, m)`. In the kit: `meter.lamp` warms and cools through a
spring (a lag on its `Glow`), and `flicker` (an `options` flag on `meter.lamp`
and the VU faces with a lamp) makes a filament's light waver. Keep motion
optional in the same way as light: a component must read right at rest.
Anything always moving (flicker, LFOs) keeps hosts drawing, so make it opt-in.
  a combined curve reads every item's inputs; a per-item curve (in a
  `Repeat`) reads only its own.
- **Collections** (`viz.eq_graph`): items are a `Repeat` template over an
  indexed family (`<ns>/b{n}_*`, shown while `b{n}_used`), not N copies.
  The plot area is a `Canvas` (double-click adds, marquee, graph menu);
  per-item editing is one context `Popover` and one context `Menu`
  following the active item; names are text params edited in a
  `TextField`. Floating panels don't use `emissive` (light composites over
  them); the `glow` frame at the bottom of the traces is where a GPU glow
  layer goes. See `docs/eq-parity.md`.

### Lit visualizers

processor's panel shaders as components — `viz.comp_scope`,
`viz.delay_taps`, `viz.reverb_space`, `viz.mod_lfo` and the `viz.eq_glow`
layer (`viz::lit`; params and feeds from `viz::lit_catalog`):

- **Structure**: a root (the rack's `#080808` ground, clipped); a `light`
  frame carrying the built-in shader (`viz.comp`, `viz.delay`, …) with its
  `inputs` bound to the component's params and feeds; inside it the vector
  fallback — processor's `paint_*` rebuilt from `Plot` / `Curve` controls
  (unit-space series of `delay.taps`, `reverb.tail`, `mod.lfo`); after it,
  what processor draws as geometry over its shader in both paths (the
  compressor's threshold rule, a `Handle` you drag; a time lane's beat grid
  is ruled by the shader itself, since processor's two painters window it
  differently).
- **Fallback fidelity**: one style per slot, so per-element alpha and size
  are bucketed (a tap's bar by loudness, a reflection by age, a shimmer by
  brightness) and gradients that travel (the delay sweep's trail, the
  shimmer's band) are drawn in slices.
- **Colour**: `color=#hex` (the lane colour; processor's defaults
  otherwise: comp grey, delay `#3b82f6`, reverb `#a78bfa`, modulation
  `#22d3ee` — `#f472b6` for motion).
- **Families/engines** are choice params (`family`, `engine`): an engine
  maps its own style list onto them (processor's `family_of_style`).

## Looking at it

```sh
nix develop . -c cargo run --release -p frame-cli --features anyrender --bin kit_sheet -- --family knob --fx gpu --out kit.png [--scale 2]
nix develop . -c cargo run -p frame-cli --bin kit_sheet -- --swatch --out swatch.png   # paint helpers
```

Each family sheet ends with a **variations** row: the `Place` options at
work (dark ink on a light panel, a tight box, a stepped knob, a concentric
pair, a GR VU at rest, a reversed GR ladder, a reversed bank with long
legends, a momentary footswitch).

Compare against the source kit's own sheets
(`processor/libs/ui/fts-audio-ui/tests/{knob_sheet,vu_sheet}.rs`).
