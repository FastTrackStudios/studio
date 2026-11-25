# studio-protools-theme

A grey console look for Studio's views, in the style of Avid Pro Tools: the [arrangement](https://github.com/FastTrackStudios/arrangement-view) and its track headers, the [mixer](https://github.com/FastTrackStudios/mixer-view), and the MIDI editor. It is [Frame](https://github.com/FastTrackStudios/frame) documents that implement daw's theme spec ([daw#52](https://github.com/FastTrackStudios/daw/issues/52), `daw-theme`), with a dark and a classic theme.

It's a look-alike in style only. Nothing in it is taken from Avid or from the themes it was measured against: no logos, icons, fonts, images or other files. Its type is Inter, which every Frame renderer bundles. Avid and Pro Tools are trademarks or registered trademarks of Avid Technology, Inc.; this pack is not affiliated with or endorsed by them.

## Themes

| Theme | Mode | What it is |
|---|---|---|
| `protools-dark` | `dark` | the current look: charcoal headers `#3a3a3a` with dark name plates `#252525`, playlists `#4e4e4e` on `#292929`, a `#242424` ruler |
| `protools-classic` | `classic` | the lighter grey of the versions before it: headers `#8d8d8d`, playlists `#b6b6b6` on `#868686`, a `#505050` ruler |

Both are the same documents in two modes (`pack.styx`). The mixer is nearly the same in both, as it is in Pro Tools; the edit window is where they part.

## Use

In Studio, with its actions:

```text
THEME_INSTALL {"link": "FastTrackStudios/studio-protools-theme"}
THEME_SET     {"pack": "protools", "theme": "protools-dark"}
THEME_CHECK   {}
```

Or for one run, by its files:

```sh
FTS_THEME=themes/protools FTS_THEME_MODE=classic Studio
```

Check it against the spec with daw's `daw-theme` command:

```sh
daw-theme check .
```

## What it looks like

- **The mixer strip**, top to foot: a bar of the track's colour; INSERTS A–E and SENDS A–E as five slots each on charcoal `#313131`; the I/O selectors as light drop-downs with black type; then, on the lower section `#6f6f6f`, the pan knob with its green light walking the rim, a black readout reading `pan 0` or `‹40` in green, input monitoring and record, solo and mute, the fader with its scale beside a two-bar meter in an olive well, the volume readout, and the name on a grey plate `#9a9a9a`, white with blue type when selected. A short strip drops the sends, then the inserts.
- **The track header**: a bar of the track's colour down the left, the name centred on a plate, then record, input, solo and mute. A taller track adds the volume and pan readouts, then the output and the inserts. A short track puts the buttons beside the name. A two-bar meter runs down the right.
- **Buttons** are grey `#636363` with a one-pixel dark outline and a highlight along the top, white glyphs. Lit, they take Pro Tools' colours with a dark glyph: solo yellow `#cab824`, mute orange `#ce6c25`, record red `#ce3225` (dark red `#4d3835` when off), input green `#4cbc2a`, auto orange `#e5994c`.
- **Readouts** are black with green figures (`#9be31d`).
- **Clips** take the track's colour, deep under a bright waveform in `dark` and pale under a saturated one in `classic`. The sixteen-hue clip palette is the bright row of Pro Tools' track colour palette.
- **The MIDI editor** keeps Pro Tools' light note area in both modes: rows `#b7b7b7`, black-key rows a step darker, blue-grey grid lines `#7f98b0`, notes in one blue `#253cf1`. The keyboard is light with dark black keys carrying their names; the ruler is blue-grey with white bar numbers; the velocity lane is light, its name in a selector on a dark column. Counters in the tool bar are black with green figures, and an active tool is blue.

## Files

| File | What it draws | Used by |
|---|---|---|
| `themes/protools/tokens.frame` | every colour, size and font, in `dark` and `classic` | every view |
| `themes/protools/controls.frame` | the shared controls: `mute`, `solo`, `arm`, `monitor`, `phase`, `pan` (the knob), `volume` (the readout), `fx`, `routing` | every view |
| `themes/protools/tcp.frame` | `tcp.row`, the track header | arrangement |
| `themes/protools/strip.frame` | `mcp.strip`, `mcp.rackback`, `mcp.step` | mixer |
| `themes/protools/editor.frame` | the twelve `editor.*` pages: bar, button, keys, key, keygroup, lane, ruler, tick, marker, strip, menu, menuitem | MIDI editor |

The rails and the FX rack have no page here, so the views draw them natively in this pack's colours.

## How it was measured

Three kinds of reference, read for colour and proportion only. None of their images or files are in this repository; every part here is drawn again as vector Frame parts.

- **Pro Tools' own figures**, from Avid's published "What's New" notes and Reference Guide: the Edit and Mix windows in the Dark theme (2020.11, 2021.6), in the Classic theme (2018.1, 2020.3, whose Mix window figure is at 1x and gave the strip's proportions: 80 px wide, 15 px slots on an 18 px pitch, 16 px buttons), and the MIDI Editor window (2025.12).
- **Pro Tools' track colour palette**: the clip palette is sixteen of its bright row's twenty-three hues.
- **Two Pro Tools-style REAPER themes**, decoded for their palettes and measured for their controls: ReaTools (V2.49, its Dark and Classic variants) for the lit button colours, the Classic ruler and rows and the MIDI grid; and ProTools 12.5_MC-edit (by daeavelwyn, on the REAPER stash) for the strip and header proportions.

### What it takes from its references, and what it does not

It takes: the palette (charcoal and mid-grey sections, the light I/O drop-downs, the lit faces, green-on-black readouts, the olive meter well, the name plate and its white selected state), the order of the strip and the header, and the proportions of slots, buttons and the strip.

It does not take: Avid's icons (the tool bar's tool glyphs, the automation and elastic icons), its fonts, its fader-cap art, its knob art, or any bitmap from the REAPER themes. Those are drawn here as plain shapes, so the look is close at a glance and differs up close.

What the pack cannot show, because Studio's views do not hand a page the value (see the spec proposals below): the plug-in name in each insert slot (a slot shows its letter when it holds one), send names and levels, the automation mode (`auto read`), the playlist and track view selectors, the group selector, and the input and output bus names (the output reads `Main` or `no output`).

## Spec proposals

Written up for daw-theme, not implemented by this pack:

- **Inserts and sends by slot**: `insert.<n>.name`, `insert.<n>.bypassed`, `send.<n>.name`, `send.<n>.level`, `send.<n>.muted` (n = 0–9, A–J), so a strip can name what is in each slot rather than count them.
- **Automation mode**: `track.automation` (off, read, touch, latch, write, trim) and a token per mode's colour, for the AUTO selector and the header's mode button.
- **Track view and playlist**: `track.view` (waveform, volume, pan, …) and `track.playlist`, for the header's two selectors.
- **Bus names**: `track.output_name` beside `track.input`.
- **Notes by velocity**: `editor.velocity_low`, `editor.velocity_high` and a switch to colour notes by velocity instead of by pitch class, the way Pro Tools can.
- **Drum lane bands**: `editor.band_kick` … `editor.band_other` (or the bands derived from `editor.drum_*` over `editor.row_a`). Today the drum editor's family bands are fixed dark colours, so they stay dark under this pack's light note area.

## Licence

GPL-3.0-or-later.
