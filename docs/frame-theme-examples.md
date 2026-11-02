# Example themes: Workbench, Control Room, Graphite

Three example Studio themes. Each is a set of Frame markup files layered over
`themes/console/`. They show how far a theme can move the mixer strip
(`mcp.strip`, `mcp.rackback`, `mcp.step`) and the track row (`tcp.row`) from
the console look, with no change to Rust code:

| Theme | Folder | Inspired by | In one line |
|---|---|---|---|
| **Workbench** | `apps/session/ui/themes/workbench/` | REAPER's utilitarian default look | Dense, flat, bevelled workbench grey; name at the top, everything labelled, square keys |
| **Control Room** | `apps/session/ui/themes/control-room/` | the sectioned mix and edit windows of Pro Tools | Cool slate console in labelled sections; raised keys, graduated dials, metallic fader, light name plate at the foot |
| **Graphite** | `apps/session/ui/themes/graphite/` | Logic Pro's modern mixer and track headers | Soft rounded graphite cards and pills; the track's colour fills the name plate and marks the fader cap |

"Inspired by" means the general composition and mood only. Every shape,
colour value and layout here was drawn for this repo. None of them copies
those products' artwork, icons, bitmaps or theme files, and none claims to
match them.

## Using them

`FTS_THEME` takes the layers base first, separated by `:` (`skin::from_env`):

```sh
FTS_THEME=apps/session/ui/themes/console:apps/session/ui/themes/workbench   <studio binary>
THEME=console:control-room just mixer-golden        # the mixer on the golden session
FTS_THEME=apps/session/ui/themes/console:apps/session/ui/themes/graphite \
  FTS_BENCH_MIXER=/tmp/graphite-mixer.png ./target/release/bench /tmp/fts-orchestral.rpp
```

Each theme replaces only what it changes. The layering rules are in
`crates/frame/src/theme/runtime/layer.rs` in FastTrackStudios/frame:

- `tokens.frame` replaces the colour tokens by name.
- `controls.frame` replaces the components `mute`, `solo`, `arm`, `pan`,
  `volume`, `fx` and `fx.compact`. `routing`, `monitor` and `phase` are still
  the console's, recoloured by the tokens.
- `strip.frame` replaces `mcp.strip`, `mcp.rackback` and `mcp.step`.
- `tcp.frame` replaces `tcp.row`.
- `rails`, `rail.button`, `rack.*` and the `editor.*` roles are still the
  console's, recoloured by the tokens.

Files reload while the app runs (`skin::poll`, every 250 ms). If an edit does
not compile, the last good theme stays on screen.

In the Session dock's panel menu, open **Theme editor** to preview edits to
the current theme. The editor opens the combined theme in memory; it does not
save changes to the source files. If the theme changes outside the editor,
close and reopen the panel before continuing.

## Design briefs

### Workbench (REAPER-inspired)

- **Intent:** a tool you live in for ten hours. It favours density and
  legibility over polish, labels everything, and draws nothing that does not
  carry state.
- **Composition (strip):** colour band and name plate at the **top**, then
  the FX well, routing beside pan, mute and solo **side by side**, then arm
  and monitor (record view). Below them is a flat fader with a separate wide
  two-channel meter to its right and a dB scale between the two. A volume
  readout that is **always on** sits at the foot, with the track number.
- **Composition (row):** one dense line: colour rail with the number, then
  arm, a sunken name field, a tight cluster of square M, S, FX and routing
  keys, and two small knobs. A tall, armed row adds an `IN` field on a
  second line.
- **Geometry:** square, radius 0–1 px, a 1 px bevel (light top, dark
  bottom), sunken wells, 4 px gutters.
- **Components:** flat plates that light in the state colour; knobs are a
  sunken well with a thin travel ring and a value arc; the fader cap is a
  flat light slab with a dark centre line.
- **Typography:** Saira at width 85 (semi-condensed), weight 600–700, for
  names and labels. JetBrains Mono for numbers (scale, readout, input).
- **Colour:** neutral mid-greys (`#2f3236` ground, `#454950` keys), a teal
  accent (`#4fc3a1`), red mute, yellow solo, green/yellow/red meter steps.

### Control Room (Pro Tools-inspired)

- **Intent:** a broadcast or post console. It reads as hardware: labelled
  sections, raised keys, a metallic fader, and the name on a plate at the
  bottom where an engineer's eye returns.
- **Composition (strip):** **INSERTS**, **SENDS · I/O** (routing beside a
  record-input display) and **PAN** (dial beside an L/C/R numeric readout)
  sections, each under a small-caps header. Then record and monitor, then
  **solo before mute**. Below them is a tall metallic fader with the meter to
  its right and a dB scale beside it, then a green numeric gain readout, and
  at the foot a light name plate over the track's colour band with the
  number in it.
- **Composition (row):** laid out like an edit-window header. A light name
  plate is on line one, with the R, S and M keys and the inserts slot under
  it on line two (inline after the name on short rows). Graduated volume and
  pan dials sit at the right with numeric readouts, and the record input is
  on a third line.
- **Geometry:** radius 2 px keys with a dark rim and a lit top half, a
  34 px tall fader cap, section headers 11 px tall.
- **Components:** keys print dark ink on slate, and the ink stays dark when
  a key lights. Dials have seven tick marks and a value arc; the fader cap is
  a static metallic gradient with grip lines, and its centre line lights in
  the accent colour while you drag it.
- **Typography:** Saira (normal width) at weight 700–800, uppercase with
  letter-spacing for the section headers. JetBrains Mono for every number
  (pan, gain, scale, input).
- **Colour:** cool slate (`#2b3442` ground, `#323c4c` sections), a sky-blue
  accent (`#5aa9e6`), yellow solo, orange mute, red record, a green readout,
  and a light plate (`#d5dce6`).

### Graphite (Logic-inspired)

- **Intent:** calm and modern. Each channel is a soft card, and the track's
  own colour does the identifying.
- **Composition (strip):** a rounded card per channel with gaps between
  cards. From the top: number, an **Audio FX** pill, routing, a **large
  36 px pan dial**, R and monitor pills, and a **pill gain readout above the
  fader**. Then a light rounded fader cap **striped in the track's colour**
  beside slim rounded meters. **M and S pills sit below the fader**, and a
  large name plate **filled with the track's colour** sits at the foot.
- **Composition (row):** a rounded card per track with a colour pill at its
  left edge and the name in medium Inter. M, S and R pills are grouped at the
  right, before two compact dials. Tall rows add an Audio FX pill, routing
  and the input.
- **Geometry:** radius 7–8 cards, fully rounded pills (`radius = h / 2`),
  rounded meter bars. Dials are drawn at any size from the instance rect
  (`arc(pw / 2, ph / 2, …)`), so one component serves the 36 px strip dial
  and the 24 px row dial.
- **Typography:** Inter at weight 500–650. The name plate's ink is picked
  for contrast against the track's colour (`ink()`).
- **Colour:** graphite (`#1f2023` ground, `#2a2b2f` cards), a periwinkle
  accent (`#7aa2ff`), blue mute, yellow solo, red record.

## Coverage: complete and planned

The Captain's direction is that themes cover five views as one system. These
examples cover what the runtime can theme today.

| View | Covered by these themes now | Still native or console, recoloured by tokens | Planned in |
|---|---|---|---|
| **Mixer** | Strip below the rack, rack ground, folder steps, every strip control (fader, meter, pan, M, S, arm, monitor, FX, routing, clip, name, folder) | The Tone rack units (console `rack.*`, recoloured). Live and touch strips use the theme's strip through the console's `strip.live`/`strip.touch` tokens, which the examples inherit; they were not checked separately | STA-680 (paint view), STA-687 |
| **Track Control Panel** | Full-panel rows: tint, rail, number, icon slot, name, folder, M, S, arm, volume, pan, FX, routing, phase, input | Compact rows, lane pills, automation button, row meter, transport bar | STA-687, STA-690 |
| **Arrangement View** | The TCP rows beside the lanes (above) and the rails | Lanes, grid, ruler, items, waveforms, MIDI, fades, cursors: no `arrange.*` roles exist yet | **STA-680** (`arrange.lane/item/ruler`, `%wave`/`%notes` slots); then these themes get their arrangement layer |
| **MIDI Editor** | Nothing theme-specific; the console's `editor.*` roles draw it, recoloured by the tokens | Notes, grid, velocity and CC lanes | STA-688, STA-690 |
| **Audio Editor** | As the MIDI Editor | Waveform backdrop and markers | STA-688, STA-690 |

**Full coverage is not claimed.** The Mixer is themed end to end below the
rack. The Arrangement's lanes and items wait for STA-680's roles, and the
Track Control Panel's compact rows wait for STA-687.

## Hit testing and accessibility

- Every interactive element carries the same `@name` targets as the console
  (`mute`, `solo`, `arm`, `monitor`, `fader`, `clip`, `pan`, `fx`,
  `routing`, `name`, `folder` in the strip; `volume` and `phase` in the
  row). So moving a control moves where it is clicked, and no control is
  lost.
- `skin::examples` checks target sets and hit testing for the layered themes.
- Semantics stay with the host (see `docs/frame-theme-contract.md`).

## Asset licences

| Asset | Used by | Source | Licence |
|---|---|---|---|
| All vector artwork, layouts and colour values in `themes/workbench/`, `themes/control-room/`, `themes/graphite/` | all three | Original, drawn for this repo (STA-689) | GPL-3.0-or-later (the repo's licence) |
| Inter (variable) | Graphite; default face | rsms/inter, bundled in Frame at `assets/inter/` | SIL OFL 1.1 (`OFL.txt` beside it) |
| Saira (variable, width 50–125) | Workbench, Control Room | google/fonts `ofl/saira`, bundled in Frame at `assets/saira/` | SIL OFL 1.1 |
| JetBrains Mono (variable) | Workbench, Control Room | google/fonts `ofl/jetbrainsmono`, bundled in Frame at `assets/jetbrains-mono/` | SIL OFL 1.1 |

No raster images, no third-party icons, and no assets taken from REAPER,
Pro Tools or Logic Pro. The fonts are the ones frame already bundles; no new
font files were added.
