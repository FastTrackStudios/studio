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
| `Needle` | anything that rotates with the value (knob pointer/body index, VU needle). Rotates about the **centre of its parent** — so put it inside a square frame centred on the pivot. Draw it at the **centre of travel** (pointing up for a standard 300° knob). |
| `ValueArc` | an arc drawn from start of travel to the value |
| `Fill` / `Thumb` / `PeakHold` | bars and sliders (drawn at full extent / top) |
| `Value` / `Label` | readout text / legend text (an authored legend wins) |
| `Led` | lamps: full opacity when on, 20% when off (for `ButtonMode::Select` banks, when *its* option is selected) |

Nothing else is special. A placed component can be restyled or taken apart
like any other nodes.

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
  travel); needle meters → `ControlKind::Gauge { sweep_degrees, floor_db, ceiling_db }`;
  bar meters → `ControlKind::Meter`; buttons → `ControlKind::Button { mode }`
  (Toggle / Momentary / Trigger / `Select { index, count }` for banks);
  footswitches → `Footswitch`; faders/levers → `Fader`; rotary selectors →
  `Knob` over a choice param (stepped).
- **Paint** with `crate::draw` helpers: `linear(angle, stops)` (CSS angles),
  `radial_at(cx, cy, r, stops)` (unit-square hot-spot), `conic(start, stops)`
  (brushed metal), `drop_shadow`/`inner_shadow`, geometry `polar`, `sector`,
  `lobed`, `regular`, `polygon`, `rotated`. Angles: degrees, 0 = up, clockwise.
- **Descriptions** are for a model choosing a component: what it looks like,
  which unit it's from, when to use it. One or two sentences.

## Looking at it

```sh
nix develop . -c cargo run -p frame-cli --bin kit_sheet -- --out kit.png [--family knob] [--scale 2]
nix develop . -c cargo run -p frame-cli --bin kit_sheet -- --swatch --out swatch.png   # paint helpers
```

Compare against the source kit's own sheets
(`processor/libs/ui/fts-audio-ui/tests/{knob_sheet,vu_sheet}.rs`).
